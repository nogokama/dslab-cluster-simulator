use std::collections::VecDeque;

use rustc_hash::FxHashMap;

use dslab_core::Id;
use dslab_scheduling::{
    config::sim_config::HostConfig,
    scheduler::{HostAvailableResources, Scheduler, SchedulerContext},
    workload_generators::events::{CollectionRequest, ExecutionRequest, ResourcesPack},
};

#[derive(Clone)]
pub struct ExecutionInfo {
    id: u64,
    resources: ResourcesPack,
    user: Option<String>,
}

fn tetris_function(pack_a: &ResourcesPack, pack_b: &ResourcesPack) -> f64 {
    let a_len = (pack_a.cpu as f64).hypot(pack_a.memory as f64);
    let b_len = (pack_b.cpu as f64).hypot(pack_b.memory as f64);
    if a_len == 0. || b_len == 0. {
        return 0.;
    }
    (pack_a.cpu as u64 * pack_b.cpu as u64 + pack_a.memory * pack_b.memory) as f64 / a_len / b_len
}

pub struct FairTetrisScheduler {
    hosts: Vec<HostConfig>,
    queues: FxHashMap<Option<String>, VecDeque<ExecutionInfo>>,
    fair_shares: FxHashMap<String, f64>,

    total_cluster_resources: ResourcesPack,
    user_resources: FxHashMap<String, ResourcesPack>,

    collection_id_to_user: FxHashMap<u64, String>,

    executions: FxHashMap<u64, ExecutionInfo>,

    scheduled: u64,
    fair_fraction: f64,
}

impl FairTetrisScheduler {
    pub fn new(fair_fraction: f64) -> FairTetrisScheduler {
        assert!((0. ..=1.).contains(&fair_fraction));

        let mut queues = FxHashMap::default();
        queues.insert(None, VecDeque::new());
        FairTetrisScheduler {
            hosts: Vec::new(),
            queues,
            fair_shares: FxHashMap::default(),
            total_cluster_resources: ResourcesPack::default(),
            user_resources: FxHashMap::default(),
            collection_id_to_user: FxHashMap::default(),
            executions: FxHashMap::default(),
            fair_fraction,
            scheduled: 0,
        }
    }

    fn schedule(&mut self, ctx: &SchedulerContext, host_id: Id, mut host_resources: ResourcesPack) {
        while let Some(execution) = self.take_most_priority_execution(&host_resources) {
            if execution.resources.fit_into(&host_resources) {
                host_resources.subtract(&execution.resources);
                if let Some(user) = execution.user.clone() {
                    self.user_resources
                        .get_mut(&user)
                        .unwrap()
                        .add(&execution.resources);
                    self.update_user_share(user);
                }
                ctx.schedule_one_host(host_id, execution.id);
                self.scheduled += 1;
            } else {
                self.queues
                    .get_mut(&execution.user)
                    .unwrap()
                    .push_front(execution);
                break;
            }
        }
    }

    fn take_most_priority_execution(&mut self, resources: &ResourcesPack) -> Option<ExecutionInfo> {
        let mut users_priority = self
            .get_sorted_users_by_share()
            .into_iter()
            .filter(|user| !self.queues.get(&Some(user.clone())).unwrap().is_empty())
            .collect::<Vec<_>>();

        let fair_len = (1. - self.fair_fraction) * users_priority.len() as f64;
        let choose_from_len = std::cmp::max(1, fair_len.ceil() as usize);

        users_priority.truncate(choose_from_len);

        if users_priority.is_empty() {
            return None;
        }

        users_priority.sort_by(|a, b| {
            let a_execution = self.queues.get(&Some(a.clone())).unwrap().front().unwrap();
            let b_execution = self.queues.get(&Some(b.clone())).unwrap().front().unwrap();

            tetris_function(resources, &b_execution.resources)
                .partial_cmp(&tetris_function(resources, &a_execution.resources))
                .unwrap()
        });

        self.queues
            .get_mut(&Some(users_priority[0].clone()))
            .unwrap()
            .pop_front()
    }

    fn get_sorted_users_by_share(&self) -> Vec<String> {
        let mut users: Vec<_> = self.fair_shares.iter().collect();
        users.sort_by(|a, b| a.1.partial_cmp(b.1).unwrap());
        users.into_iter().map(|(user, _)| user.clone()).collect()
    }

    fn update_user_share(&mut self, user: String) {
        let user_resources = self.user_resources.get(&user).unwrap();
        let user_cpu_share =
            (user_resources.cpu as f64) / (self.total_cluster_resources.cpu as f64);
        let user_memory_share =
            (user_resources.memory as f64) / (self.total_cluster_resources.memory as f64);
        self.fair_shares.insert(
            user,
            if user_cpu_share > user_memory_share {
                user_cpu_share
            } else {
                user_memory_share
            },
        );
    }
}

impl Scheduler for FairTetrisScheduler {
    fn on_execution_finished(
        &mut self,
        ctx: &SchedulerContext,
        execution_id: u64,
        hosts: Vec<HostAvailableResources>,
    ) {
        assert_eq!(hosts.len(), 1);

        let execution = self.executions.remove(&execution_id).unwrap();
        if let Some(user) = execution.user {
            let user_resources = self.user_resources.get_mut(&user).unwrap();
            user_resources.cpu -= execution.resources.cpu;
            user_resources.memory -= execution.resources.memory;

            self.update_user_share(user);
        }

        self.schedule(ctx, hosts[0].host_id, hosts[0].resources);
    }

    fn on_execution_request(&mut self, _ctx: &SchedulerContext, request: ExecutionRequest) {
        let execution_id = request.id.unwrap();
        let cpu = request.resources.cpu_per_node;
        let memory = request.resources.memory_per_node;
        let user = if let Some(collection_id) = request.collection_id {
            self.collection_id_to_user.get(&collection_id).cloned()
        } else {
            None
        };

        let execution_info = ExecutionInfo {
            id: execution_id,
            resources: ResourcesPack::new_cpu_memory(cpu, memory),
            user: user.clone(),
        };
        self.queues
            .get_mut(&user)
            .unwrap()
            .push_back(execution_info.clone());

        self.executions.insert(execution_id, execution_info);
    }

    fn on_host_resources(&mut self, ctx: &SchedulerContext, host_id: Id, resources: ResourcesPack) {
        self.schedule(ctx, host_id, resources);
    }

    fn on_host_added(&mut self, host: HostConfig) {
        self.total_cluster_resources.cpu += host.cpus;
        self.total_cluster_resources.memory += host.memory;
        self.hosts.push(host);
    }

    fn on_collection_request(
        &mut self,
        _ctx: &SchedulerContext,
        collection_request: CollectionRequest,
    ) {
        if let Some(user) = collection_request.user {
            self.collection_id_to_user
                .insert(collection_request.id.unwrap(), user.clone());
            self.fair_shares.insert(user.clone(), 0.);
            self.user_resources
                .insert(user.clone(), ResourcesPack::default());
            self.queues.insert(Some(user), VecDeque::new());
        }
    }
}
