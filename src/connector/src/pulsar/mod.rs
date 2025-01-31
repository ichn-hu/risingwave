// Copyright 2022 Singularity Data
//
// Licensed under the Apache License, Version 2.0 (the "License");
// you may not use this file except in compliance with the License.
// You may obtain a copy of the License at
//
// http://www.apache.org/licenses/LICENSE-2.0
//
// Unless required by applicable law or agreed to in writing, software
// distributed under the License is distributed on an "AS IS" BASIS,
// WITHOUT WARRANTIES OR CONDITIONS OF ANY KIND, either express or implied.
// See the License for the specific language governing permissions and
// limitations under the License.

mod admin;
pub(crate) mod enumerator;
pub mod source;
pub mod split;
mod topic;

pub use enumerator::*;
pub use split::*;

const PULSAR_CONFIG_TOPIC_KEY: &str = "pulsar.topic";
const PULSAR_CONFIG_ADMIN_URL_KEY: &str = "pulsar.admin.url";
