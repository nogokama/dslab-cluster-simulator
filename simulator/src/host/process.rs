use std::rc::Rc;

use dslab_compute::multicore::CoresDependency;
use dslab_core::Id;

use super::cluster_host::ClusterHost;

pub type ProcessId = u64;

pub struct HostProcessInstance {
    pub id: ProcessId,
    pub compute_allocation_id: u64,
    pub host: Rc<ClusterHost>,
}

impl HostProcessInstance {
    pub async fn sleep(&self, time: f64) {
        self.host.sleep(time).await;
    }

    pub async fn run_compute(&self, compute_work: f64, cores_dependency: CoresDependency) {
        self.host
            .run_compute(compute_work, self.compute_allocation_id, cores_dependency)
            .await;
    }

    pub async fn transfer_data_to_process(&self, size: f64, dst_process: ProcessId) {
        self.host.transfer_data_to_process(size, dst_process).await;
    }

    pub async fn transfer_data_from_process(&self, size: f64, src_process: ProcessId) {
        self.host
            .transfer_data_from_process(size, src_process)
            .await;
    }

    pub async fn transfer_data_to_component(&self, size: f64, component_id: Id) {
        self.host
            .transfer_data_to_component(size, component_id)
            .await;
    }

    pub async fn transfer_data_from_component(&self, size: f64, component_id: Id) {
        self.host
            .transfer_data_from_component(size, component_id)
            .await;
    }

    pub async fn write_data(&self, size: u64) -> Result<(), String> {
        self.host.write_data(size).await
    }

    pub async fn read_data(&self, size: u64) -> Result<(), String> {
        self.host.read_data(size).await
    }
}
