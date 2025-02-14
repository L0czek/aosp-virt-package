/*
 * Copyright 2023 The Android Open Source Project
 *
 * Licensed under the Apache License, Version 2.0 (the "License");
 * you may not use this file except in compliance with the License.
 * You may obtain a copy of the License at
 *
 *      http://www.apache.org/licenses/LICENSE-2.0
 *
 * Unless required by applicable law or agreed to in writing, software
 * distributed under the License is distributed on an "AS IS" BASIS,
 * WITHOUT WARRANTIES OR CONDITIONS OF ANY KIND, either express or implied.
 * See the License for the specific language governing permissions and
 * limitations under the License.
 */
package android.system.virtualizationservice;

/** CPU options that will be used for the VM's Vcpus. */
@RustDerive(Clone=true)
parcelable CpuOptions {
    @RustDerive(Clone=true, PartialEq=true)
    union CpuTopology {
        /** Number of Vcpus to boot the VM with. */
        int cpuCount = 1;

        /** Match host number of Vcpus to boot the VM with. */
        boolean matchHost;
    }

    CpuTopology cpuTopology;
}
