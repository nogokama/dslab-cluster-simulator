use std::{cell::RefCell, rc::Rc};

use futures::{select, FutureExt};

use dslab_compute::multicore::{CompFinished, CompStarted, Compute, CoresDependency};
use dslab_core::{cast, log_debug, EventHandler, Id, SimulationContext};
use dslab_network::{DataTransferCompleted, Network};
use dslab_storage::disk::Disk;
use dslab_storage::events::{
    DataReadCompleted, DataReadFailed, DataWriteCompleted, DataWriteFailed,
};
use dslab_storage::storage::Storage;

use crate::monitoring::Monitoring;
use crate::storage::SharedInfoStorage;

use super::process::ProcessId;

pub struct ClusterHost {
    pub compute: Rc<RefCell<Compute>>,
    pub network: Option<Rc<RefCell<Network>>>,
    pub disk: Option<Rc<RefCell<Disk>>>,
    shared_info_storage: Rc<RefCell<SharedInfoStorage>>,
    group_prefix: Option<String>,
    monitoring: Rc<RefCell<Monitoring>>,
    ctx: SimulationContext,
}

impl ClusterHost {
    pub fn new(
        compute: Rc<RefCell<Compute>>,
        network: Option<Rc<RefCell<Network>>>,
        disk: Option<Rc<RefCell<Disk>>>,
        shared_info_storage: Rc<RefCell<SharedInfoStorage>>,
        monitoring: Rc<RefCell<Monitoring>>,
        group_prefix: Option<String>,
        ctx: SimulationContext,
    ) -> ClusterHost {
        ClusterHost {
            compute,
            network,
            disk,
            shared_info_storage,
            monitoring,
            group_prefix,
            ctx,
        }
    }

    pub fn id(&self) -> Id {
        self.ctx.id()
    }

    pub async fn sleep(&self, time: f64) {
        self.ctx.sleep(time).await;
    }

    pub async fn run_compute(
        &self,
        compute_work: f64,
        compute_allocation_id: u64,
        cores_dependency: CoresDependency,
    ) {
        let req_id = self.compute.borrow_mut().run_on_allocation(
            compute_work,
            compute_allocation_id,
            cores_dependency,
            self.ctx.id(),
        );

        log_debug!(
            self.ctx,
            "running flops: id={}, flops={}",
            req_id,
            compute_work
        );
        self.ctx.recv_event_by_key::<CompStarted>(req_id).await;

        self.ctx.recv_event_by_key::<CompFinished>(req_id).await;

        log_debug!(
            self.ctx,
            "completed flops: id={}, flops={}",
            req_id,
            compute_work
        );
    }

    pub async fn transfer_data_to_process(&self, size: f64, dst_process: ProcessId) {
        let dst_host = self.shared_info_storage.borrow().get_host_id(dst_process);
        self.transfer_data(size, self.ctx.id(), dst_host).await;
    }

    pub async fn transfer_data_from_process(&self, size: f64, src_process: ProcessId) {
        let src_host = self.shared_info_storage.borrow().get_host_id(src_process);
        self.transfer_data(size, src_host, self.ctx.id()).await;
    }

    pub async fn transfer_data_to_component(&self, size: f64, component_id: Id) {
        self.transfer_data(size, self.ctx.id(), component_id).await;
    }

    pub async fn transfer_data_from_component(&self, size: f64, component_id: Id) {
        self.transfer_data(size, component_id, self.ctx.id()).await;
    }

    async fn transfer_data(&self, size: f64, src: Id, dst: Id) {
        let network = self
            .network
            .as_ref()
            .expect("network must be configured to call network operations");

        let req_id = network
            .borrow_mut()
            .transfer_data(src, dst, size, self.ctx.id());

        self.ctx
            .recv_event_by_key::<DataTransferCompleted>(req_id as u64)
            .await;
    }

    pub async fn write_data(&self, size: u64) -> Result<(), String> {
        let req_id = self
            .disk
            .as_ref()
            .expect("disk must be configured to call disk operations")
            .borrow_mut()
            .write(size, self.ctx.id());

        select! {
            _ = self.ctx.recv_event_by_key::<DataWriteCompleted>(req_id).fuse() => {
                Result::Ok(())
            }
            failed = self.ctx.recv_event_by_key::<DataWriteFailed>(req_id).fuse() => {
                Result::Err(failed.data.error)
            }
        }
    }

    pub async fn read_data(&self, size: u64) -> Result<(), String> {
        let req_id = self
            .disk
            .as_ref()
            .expect("disk must be configured to call disk operations")
            .borrow_mut()
            .read(size, self.ctx.id());

        select! {
            _ = self.ctx.recv_event_by_key::<DataReadCompleted>(req_id).fuse() => {
                Result::Ok(())
            }
            failed = self.ctx.recv_event_by_key::<DataReadFailed>(req_id).fuse() => {
                Result::Err(failed.data.error)
            }
        }
    }

    pub fn log_compute_load(&self) {
        let cpu_used =
            self.compute.borrow().cores_total() - self.compute.borrow().cores_available();
        let memory_used =
            self.compute.borrow().memory_total() - self.compute.borrow().memory_available();
        self.monitoring.borrow_mut().update_host(
            self.ctx.time(),
            self.ctx.name(),
            self.group_prefix.as_ref(),
            cpu_used,
            memory_used,
            None,
        );
    }
}

impl EventHandler for ClusterHost {
    fn on(&mut self, event: dslab_core::Event) {
        cast!(match event.data {
            CompFinished { id } => {
                println!("comp finished: {}", id);
            }
        })
    }
}
