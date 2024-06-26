use dslab_core::{cast, EventHandler, Id, SimulationContext};
use serde::Serialize;

use crate::{
    cluster::{CancelExecution, ExecutionFinished, HostInvoked, ScheduleExecution},
    cluster_events::HostAdded,
    config::sim_config::HostConfig,
    workload_generators::events::{
        CollectionRequest, CollectionRequestEvent, ExecutionRequest, ExecutionRequestEvent,
        ResourcesPack,
    },
};

#[derive(Clone, Serialize)]
pub struct HostAvailableResources {
    pub host_id: Id,
    pub resources: ResourcesPack,
}

#[derive(Default, Clone, Serialize)]
pub struct SchedulerStats {
    pub on_host_added_cnt: Option<u64>,
    pub on_host_added_time: Option<u64>,
    pub on_execution_request_cnt: Option<u64>,
    pub on_execution_request_time: Option<u64>,
    pub on_collection_request_cnt: Option<u64>,
    pub on_collection_request_time: Option<u64>,
    pub on_execution_finished_cnt: Option<u64>,
    pub on_execution_finished_time: Option<u64>,
    pub on_host_resources_cnt: Option<u64>,
    pub on_host_resources_time: Option<u64>,
}

pub trait CustomScheduler: EventHandler {
    fn name(&self) -> String;
    fn id(&self) -> Id;
    fn get_stats(&self) -> SchedulerStats {
        SchedulerStats::default()
    }
}

pub struct SchedulerContext {
    pub ctx: SimulationContext,
    cluster_id: Id,
}

impl SchedulerContext {
    pub fn new(ctx: SimulationContext, cluster_id: Id) -> Self {
        SchedulerContext { ctx, cluster_id }
    }

    pub fn schedule(&self, host_ids: Vec<Id>, execution_id: u64) {
        self.ctx.emit_now(
            ScheduleExecution {
                host_ids,
                execution_id,
            },
            self.cluster_id,
        );
    }
    pub fn schedule_one_host(&self, host_id: Id, execution_id: u64) {
        self.ctx.emit_now(
            ScheduleExecution {
                host_ids: vec![host_id],
                execution_id,
            },
            self.cluster_id,
        );
    }
    pub fn cancel(&self, execution_id: u64) {
        self.ctx
            .emit_now(CancelExecution { execution_id }, self.cluster_id);
    }
}

pub trait Scheduler {
    fn on_host_added(&mut self, host: HostConfig);
    fn on_execution_request(&mut self, ctx: &SchedulerContext, request: ExecutionRequest);
    fn on_collection_request(
        &mut self,
        ctx: &SchedulerContext,
        collection_request: CollectionRequest,
    );
    fn on_execution_finished(
        &mut self,
        ctx: &SchedulerContext,
        execution_id: u64,
        hosts: Vec<HostAvailableResources>,
    );
    fn on_host_resources(&mut self, ctx: &SchedulerContext, host_id: Id, resources: ResourcesPack);
}

pub struct SchedulerInvoker<T: Scheduler> {
    scheduler: T,
    ctx: SchedulerContext,
}

impl<T: Scheduler> SchedulerInvoker<T> {
    pub fn new(scheduler: T, ctx: SimulationContext, cluster_id: Id) -> Self {
        SchedulerInvoker {
            scheduler,
            ctx: SchedulerContext { ctx, cluster_id },
        }
    }
}

impl<T: Scheduler> CustomScheduler for SchedulerInvoker<T> {
    fn id(&self) -> Id {
        self.ctx.ctx.id()
    }
    fn name(&self) -> String {
        self.ctx.ctx.name().to_string()
    }
}

impl<T: Scheduler> EventHandler for SchedulerInvoker<T> {
    fn on(&mut self, event: dslab_core::Event) {
        cast!(match event.data {
            HostAdded { host } => {
                self.scheduler.on_host_added(host);
            }
            ExecutionRequestEvent { request } => {
                self.scheduler.on_execution_request(&self.ctx, request);
            }
            ExecutionFinished {
                execution_id,
                hosts,
            } => {
                self.scheduler
                    .on_execution_finished(&self.ctx, execution_id, hosts);
            }
            CollectionRequestEvent { request } => {
                self.scheduler.on_collection_request(&self.ctx, request);
            }
            HostInvoked { id, resources } => {
                self.scheduler.on_host_resources(&self.ctx, id, resources);
            }
        })
    }
}
