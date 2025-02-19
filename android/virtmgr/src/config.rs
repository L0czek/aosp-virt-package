use crate::debug_config::DebugConfig;
use std::num::{NonZeroU16, NonZeroU32};
use std::fs::File;
use semver::VersionReq;
use binder::Strong;
use android_system_virtualizationservice_internal::aidl::android::system::virtualizationservice_internal::IBoundDevice::IBoundDevice;
use crate::aidl::Cid;
use anyhow::{anyhow, Result};
use android_system_virtualizationservice::aidl::android::system::virtualizationservice::{
    AudioConfig::AudioConfig as AudioConfigParcelable,
    DisplayConfig::DisplayConfig as DisplayConfigParcelable,
    GpuConfig::GpuConfig as GpuConfigParcelable,
    UsbConfig::UsbConfig as UsbConfigParcelable,
};
use std::sync::Arc;
use shared_child::SharedChild;
use std::thread::JoinHandle;

pub(crate) type VfioDevice = Strong<dyn IBoundDevice>;

/// Configuration for a VM to run with crosvm.
#[derive(Debug)]
pub struct CrosvmConfig {
    pub cid: Cid,
    pub name: String,
    pub bootloader: Option<File>,
    pub kernel: Option<File>,
    pub initrd: Option<File>,
    pub disks: Vec<DiskFile>,
    pub params: Option<String>,
    pub protected: bool,
    pub debug_config: DebugConfig,
    pub memory_mib: NonZeroU32,
    pub cpus: Option<NonZeroU32>,
    pub host_cpu_topology: bool,
    pub console_out_fd: Option<File>,
    pub console_in_fd: Option<File>,
    pub log_fd: Option<File>,
    pub ramdump: Option<File>,
    pub indirect_files: Vec<File>,
    pub platform_version: VersionReq,
    pub detect_hangup: bool,
    pub gdb_port: Option<NonZeroU16>,
    pub vfio_devices: Vec<VfioDevice>,
    pub dtbo: Option<File>,
    pub device_tree_overlay: Option<File>,
    pub display_config: Option<DisplayConfig>,
    pub input_device_options: Vec<InputDeviceOption>,
    pub hugepages: bool,
    pub tap: Option<File>,
    pub console_input_device: Option<String>,
    pub boost_uclamp: bool,
    pub gpu_config: Option<GpuConfig>,
    pub audio_config: Option<AudioConfig>,
    pub no_balloon: bool,
    pub usb_config: UsbConfig,
}

fn try_into_non_zero_u32(value: i32) -> Result<NonZeroU32> {
    let u32_value = value.try_into()?;
    NonZeroU32::new(u32_value).ok_or(anyhow!("value should be greater than 0"))
}

/// A disk image to pass to crosvm for a VM.
#[derive(Debug)]
pub struct DiskFile {
    pub image: File,
    pub writable: bool,
}

#[derive(Debug)]
pub struct AudioConfig {
    pub use_microphone: bool,
    pub use_speaker: bool,
}

impl AudioConfig {
    pub fn new(raw_config: &AudioConfigParcelable) -> Self {
        AudioConfig { use_microphone: raw_config.useMicrophone, use_speaker: raw_config.useSpeaker }
    }
}

#[derive(Debug)]
pub struct UsbConfig {
    pub controller: bool,
}

impl UsbConfig {
    pub fn new(raw_config: &UsbConfigParcelable) -> Result<UsbConfig> {
        Ok(UsbConfig { controller: raw_config.controller })
    }
}

#[derive(Debug)]
pub struct DisplayConfig {
    pub width: NonZeroU32,
    pub height: NonZeroU32,
    pub horizontal_dpi: NonZeroU32,
    pub vertical_dpi: NonZeroU32,
    pub refresh_rate: NonZeroU32,
}

impl DisplayConfig {
    pub fn new(raw_config: &DisplayConfigParcelable) -> Result<DisplayConfig> {
        let width = try_into_non_zero_u32(raw_config.width)?;
        let height = try_into_non_zero_u32(raw_config.height)?;
        let horizontal_dpi = try_into_non_zero_u32(raw_config.horizontalDpi)?;
        let vertical_dpi = try_into_non_zero_u32(raw_config.verticalDpi)?;
        let refresh_rate = try_into_non_zero_u32(raw_config.refreshRate)?;
        Ok(DisplayConfig { width, height, horizontal_dpi, vertical_dpi, refresh_rate })
    }
}

#[derive(Debug)]
pub struct GpuConfig {
    pub backend: Option<String>,
    pub context_types: Option<Vec<String>>,
    pub pci_address: Option<String>,
    pub renderer_features: Option<String>,
    pub renderer_use_egl: Option<bool>,
    pub renderer_use_gles: Option<bool>,
    pub renderer_use_glx: Option<bool>,
    pub renderer_use_surfaceless: Option<bool>,
    pub renderer_use_vulkan: Option<bool>,
}

impl GpuConfig {
    pub fn new(raw_config: &GpuConfigParcelable) -> Result<GpuConfig> {
        Ok(GpuConfig {
            backend: raw_config.backend.clone(),
            context_types: raw_config.contextTypes.clone().map(|context_types| {
                context_types.iter().filter_map(|context_type| context_type.clone()).collect()
            }),
            pci_address: raw_config.pciAddress.clone(),
            renderer_features: raw_config.rendererFeatures.clone(),
            renderer_use_egl: Some(raw_config.rendererUseEgl),
            renderer_use_gles: Some(raw_config.rendererUseGles),
            renderer_use_glx: Some(raw_config.rendererUseGlx),
            renderer_use_surfaceless: Some(raw_config.rendererUseSurfaceless),
            renderer_use_vulkan: Some(raw_config.rendererUseVulkan),
        })
    }
}

///
/// virtio-input device configuration from `external/crosvm/src/crosvm/config.rs`
#[derive(Debug)]
#[allow(dead_code)]
pub enum InputDeviceOption {
    EvDev(File),
    SingleTouch { file: File, width: u32, height: u32, name: Option<String> },
    Keyboard(File),
    Mouse(File),
    Switches(File),
    MultiTouchTrackpad { file: File, width: u32, height: u32, name: Option<String> },
    MultiTouch { file: File, width: u32, height: u32, name: Option<String> },
}


/// The lifecycle state which the payload in the VM has reported itself to be in.
///
/// Note that the order of enum variants is significant; only forward transitions are allowed by
/// [`VmInstance::update_payload_state`].
#[derive(Copy, Clone, Debug, Eq, Ord, PartialEq, PartialOrd)]
pub enum PayloadState {
    Starting,
    Started,
    Ready,
    Finished,
    Hangup, // Hasn't reached to Ready before timeout expires
}

/// The current state of the VM itself.
#[derive(Debug)]
pub enum VmState {
    /// The VM has not yet tried to start.
    NotStarted {
        ///The configuration needed to start the VM, if it has not yet been started.
        config: Box<CrosvmConfig>,
    },
    /// The VM has been started.
    Running {
        /// The crosvm child process.
        child: Arc<SharedChild>,
        /// The thread waiting for crosvm to finish.
        monitor_vm_exit_thread: Option<JoinHandle<()>>,
    },
    /// The VM died or was killed.
    Dead,
    /// The VM failed to start.
    Failed,
}
