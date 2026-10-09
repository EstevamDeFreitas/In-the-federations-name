use federation_sim::*;

#[test]
fn extreme_coordinates_and_maximum_timestep_interpolate_without_overflow() {
    let mut sim = Simulation::new(SimulationConfig {
        vehicles: 1,
        threads: 1,
        tick_ms: u32::MAX,
        ..Default::default()
    })
    .unwrap();
    let low = TransportPosition {
        x: -(1 << 50),
        y: -(1 << 50),
        z: -(1 << 50),
    };
    let high = TransportPosition {
        x: 1 << 50,
        y: 1 << 50,
        z: 1 << 50,
    };
    let (a, _) = sim.submit_create_transport_node(low).unwrap();
    let (b, _) = sim.submit_create_transport_node(high).unwrap();
    sim.submit_create_transport_edge(a, b, TransportMode::Road, 1, true)
        .unwrap();
    sim.tick().unwrap();
    let route = sim
        .find_route(
            RouteRequest {
                origin: a,
                destination: b,
                mode: TransportMode::Road,
            },
            &mut RoutingWorkspace::new(),
        )
        .unwrap();
    let (plan, _) = sim.submit_store_route_plan(route).unwrap();
    let id = all(&sim)[0].id;
    assign(&mut sim, id, plan, 1_000_000);
    sim.tick().unwrap();
    let partial = vehicle(&sim, id);
    assert_eq!(partial.movement_state, VehicleMovementState::FollowingRoute);
    assert_eq!(partial.edge_progress, u64::from(u32::MAX) * 1_000_000);
    for value in [partial.x, partial.y, partial.z] {
        assert!((-(1 << 50)..=1 << 50).contains(&value));
    }
    sim.tick().unwrap();
    let arrived = vehicle(&sim, id);
    assert_eq!(arrived.movement_state, VehicleMovementState::Arrived);
    assert_eq!((arrived.x, arrived.y, arrived.z), (high.x, high.y, high.z));
}

#[test]
fn parked_vehicle_rejects_plan_origin_mismatch_without_teleport() {
    let (mut sim, _, _, plan) = setup(1, 1);
    let id = all(&sim)[0].id;
    assign(&mut sim, id, plan, 100);
    sim.tick().unwrap();
    let arrived = vehicle(&sim, id);
    sim.submit_command(SimulationCommand::AssignVehicleRoute {
        vehicle_id: id,
        route_plan_id: plan,
    })
    .unwrap();
    sim.tick().unwrap();
    assert_eq!(
        sim.command_results()[0].result,
        Err(CommandError::InvalidRouteAssignment)
    );
    assert_eq!(vehicle(&sim, id), arrived);
}

fn sim(threads: usize, vehicles: usize) -> Simulation {
    Simulation::new(SimulationConfig {
        width: 8,
        height: 8,
        vehicles,
        threads,
        tick_ms: 10,
        ..Default::default()
    })
    .unwrap()
}
fn vehicle(sim: &Simulation, id: VehicleId) -> VehicleSnapshot {
    sim.snapshot(SnapshotRequest {
        vehicles: VehicleSelection::Ids(&[id]),
        ..Default::default()
    })
    .unwrap()
    .vehicles[0]
}
fn all(sim: &Simulation) -> Vec<VehicleSnapshot> {
    sim.snapshot(SnapshotRequest {
        vehicles: VehicleSelection::All,
        ..Default::default()
    })
    .unwrap()
    .vehicles
}
fn plan(sim: &Simulation, id: RoutePlanId) -> RoutePlan {
    sim.snapshot(SnapshotRequest {
        route_plans: RoutePlanSelection::Ids(&[id]),
        ..Default::default()
    })
    .unwrap()
    .route_plans
    .remove(0)
}
fn setup(
    threads: usize,
    vehicles: usize,
) -> (
    Simulation,
    [TransportNodeId; 3],
    [TransportEdgeId; 2],
    RoutePlanId,
) {
    let mut sim = sim(threads, vehicles);
    let (a, _) = sim
        .submit_create_transport_node(TransportPosition { x: 0, y: 0, z: 0 })
        .unwrap();
    let (b, _) = sim
        .submit_create_transport_node(TransportPosition { x: 100, y: 0, z: 0 })
        .unwrap();
    let (c, _) = sim
        .submit_create_transport_node(TransportPosition {
            x: 100,
            y: 100,
            z: 50,
        })
        .unwrap();
    let (ab, _) = sim
        .submit_create_transport_edge(a, b, TransportMode::Road, 999, true)
        .unwrap();
    let (bc, _) = sim
        .submit_create_transport_edge(b, c, TransportMode::Road, 1, true)
        .unwrap();
    sim.tick().unwrap();
    let route = sim
        .find_route(
            RouteRequest {
                origin: a,
                destination: c,
                mode: TransportMode::Road,
            },
            &mut RoutingWorkspace::new(),
        )
        .unwrap();
    let (id, _) = sim.submit_store_route_plan(route).unwrap();
    (sim, [a, b, c], [ab, bc], id)
}
fn assign(sim: &mut Simulation, id: VehicleId, plan: RoutePlanId, speed: u64) {
    sim.submit_command(SimulationCommand::SetVehicleNetworkSpeed {
        vehicle_id: id,
        speed_units_per_ms: speed,
    })
    .unwrap();
    sim.submit_command(SimulationCommand::AssignVehicleRoute {
        vehicle_id: id,
        route_plan_id: plan,
    })
    .unwrap();
}

#[test]
fn physical_length_is_separate_integer_xyz_and_safe_at_extremes() {
    let mut sim = sim(1, 0);
    let cases = [
        (TransportPosition { x: 100, y: 0, z: 0 }, 100),
        (TransportPosition { x: 0, y: -50, z: 0 }, 50),
        (TransportPosition { x: 30, y: 40, z: 0 }, 70),
        (TransportPosition { x: 0, y: 0, z: 25 }, 25),
        (TransportPosition::default(), 1),
        (
            TransportPosition {
                x: 1 << 50,
                y: -(1 << 50),
                z: 1 << 50,
            },
            3_u64 * (1 << 50),
        ),
    ];
    let (origin, _) = sim
        .submit_create_transport_node(TransportPosition::default())
        .unwrap();
    let mut ids = Vec::new();
    for (position, _) in cases {
        let (to, _) = sim.submit_create_transport_node(position).unwrap();
        ids.push(
            sim.submit_create_transport_edge(origin, to, TransportMode::Road, 7, true)
                .unwrap()
                .0,
        );
    }
    sim.tick().unwrap();
    let edges = sim
        .snapshot(SnapshotRequest {
            transport: Some(TransportSnapshotRequest {
                include_edges: true,
                ..Default::default()
            }),
            ..Default::default()
        })
        .unwrap()
        .transport
        .unwrap()
        .edges;
    for ((_, expected), id) in cases.into_iter().zip(ids) {
        let edge = edges.iter().find(|e| e.id == id).unwrap();
        assert_eq!(edge.physical_length, expected);
        assert_eq!(edge.base_cost, 7);
    }
    let (low, _) = sim
        .submit_create_transport_node(TransportPosition {
            x: -(1 << 50),
            y: -(1 << 50),
            z: -(1 << 50),
        })
        .unwrap();
    let (high, _) = sim
        .submit_create_transport_node(TransportPosition {
            x: 1 << 50,
            y: 1 << 50,
            z: 1 << 50,
        })
        .unwrap();
    let (id, _) = sim
        .submit_create_transport_edge(low, high, TransportMode::Rail, 1, true)
        .unwrap();
    sim.tick().unwrap();
    let snapshot = sim
        .snapshot(SnapshotRequest {
            transport: Some(TransportSnapshotRequest {
                include_edges: true,
                ..Default::default()
            }),
            ..Default::default()
        })
        .unwrap();
    assert_eq!(
        snapshot
            .transport
            .unwrap()
            .edges
            .iter()
            .find(|e| e.id == id)
            .unwrap()
            .physical_length,
        3 * (1_u64 << 51)
    );
}

#[test]
fn route_plan_storage_is_deferred_immutable_selective_and_shareable() {
    let (mut sim, nodes, edges, plan_id) = setup(1, 2);
    assert!(
        sim.snapshot(SnapshotRequest {
            route_plans: RoutePlanSelection::Ids(&[plan_id]),
            ..Default::default()
        })
        .is_err()
    );
    let ids: Vec<_> = all(&sim).iter().map(|v| v.id).collect();
    for id in &ids {
        assign(&mut sim, *id, plan_id, 1);
    }
    sim.tick().unwrap();
    let old = plan(&sim, plan_id);
    assert_eq!(old.edges, edges);
    assert_eq!(old.origin, nodes[0]);
    assert!(all(&sim).iter().all(|v| v.route_plan_id == Some(plan_id)));
    let route = sim
        .find_route(
            RouteRequest {
                origin: nodes[0],
                destination: nodes[2],
                mode: TransportMode::Road,
            },
            &mut RoutingWorkspace::new(),
        )
        .unwrap();
    sim.submit_command(SimulationCommand::StoreRoutePlan {
        route_plan_id: plan_id,
        route,
    })
    .unwrap();
    sim.tick().unwrap();
    assert_eq!(
        sim.command_results()[0].result,
        Err(CommandError::RoutePlanIdUsed(plan_id))
    );
    assert_eq!(plan(&sim, plan_id), old);
    let mut changed = old.clone();
    changed.edges.clear();
    assert_eq!(plan(&sim, plan_id), old);
    assert!(
        sim.snapshot(SnapshotRequest::default())
            .unwrap()
            .route_plans
            .is_empty()
    );
    let next = sim.reserve_route_plan_id().unwrap();
    assert!(next > plan_id);
}

#[test]
fn partial_progress_crosses_edges_and_arrives_exactly_in_xyz() {
    let (mut sim, nodes, edges, plan_id) = setup(1, 1);
    let id = all(&sim)[0].id;
    assign(&mut sim, id, plan_id, 3);
    sim.tick().unwrap();
    let first = vehicle(&sim, id);
    assert_eq!(first.current_edge, Some(edges[0]));
    assert_eq!(first.edge_progress, 30);
    assert_eq!((first.x, first.y, first.z), (30, 0, 0));
    assert_eq!(first.current_node, Some(nodes[0]));
    for _ in 0..2 {
        sim.tick().unwrap();
    }
    assert_eq!(vehicle(&sim, id).edge_progress, 90);
    sim.tick().unwrap();
    let second = vehicle(&sim, id);
    assert_eq!(second.current_edge, Some(edges[1]));
    assert_eq!(second.edge_progress, 20);
    assert_eq!((second.x, second.y, second.z), (100, 13, 6));
    assert_eq!(second.current_node, Some(nodes[1]));
    for _ in 0..5 {
        sim.tick().unwrap();
    }
    let arrived = vehicle(&sim, id);
    assert_eq!(arrived.movement_state, VehicleMovementState::Arrived);
    assert_eq!((arrived.x, arrived.y, arrived.z), (100, 100, 50));
    assert_eq!(arrived.current_node, Some(nodes[2]));
    assert_eq!(arrived.current_edge, None);
    assert_eq!(arrived.edge_progress, 0);
    assert_eq!(
        sim.movement_events(),
        [MovementEvent {
            vehicle_id: id,
            kind: MovementEventKind::VehicleArrived,
            current_node: Some(nodes[2])
        }]
    );
    sim.tick().unwrap();
    assert_eq!(vehicle(&sim, id), arrived);
    assert!(sim.movement_events().is_empty());
}

#[test]
fn large_budget_consumes_multiple_edges_in_one_tick_without_routing() {
    let (mut sim, nodes, _, plan_id) = setup(4, 1);
    let id = all(&sim)[0].id;
    assign(&mut sim, id, plan_id, 100);
    sim.tick().unwrap();
    assert_eq!(
        vehicle(&sim, id).movement_state,
        VehicleMovementState::Arrived
    );
    assert_eq!(vehicle(&sim, id).current_node, Some(nodes[2]));
    assert_eq!(sim.movement_metrics().edge_transitions, 2);
    assert_eq!(sim.movement_metrics().arrivals, 1);
}

#[test]
fn removed_future_edge_blocks_only_when_reached_and_allows_explicit_replanning() {
    let (mut sim, nodes, edges, plan_id) = setup(1, 1);
    let id = all(&sim)[0].id;
    assign(&mut sim, id, plan_id, 3);
    sim.tick().unwrap();
    sim.submit_command(SimulationCommand::RemoveTransportEdge { edge_id: edges[1] })
        .unwrap();
    sim.tick().unwrap();
    assert_eq!(
        vehicle(&sim, id).movement_state,
        VehicleMovementState::FollowingRoute
    );
    for _ in 0..2 {
        sim.tick().unwrap();
    }
    let stopped = vehicle(&sim, id);
    assert_eq!(stopped.movement_state, VehicleMovementState::NeedsRoute);
    assert_eq!(stopped.current_node, Some(nodes[1]));
    assert_eq!((stopped.x, stopped.y, stopped.z), (100, 0, 0));
    assert_eq!(
        sim.movement_events()[0].kind,
        MovementEventKind::VehicleNeedsRoute(MovementBlockReason::InvalidEdge)
    );
    sim.tick().unwrap();
    assert_eq!(vehicle(&sim, id), stopped);
    assert!(sim.movement_events().is_empty());
    sim.submit_create_transport_edge(nodes[1], nodes[2], TransportMode::Road, 5, true)
        .unwrap();
    sim.tick().unwrap();
    let route = sim
        .find_route(
            RouteRequest {
                origin: nodes[1],
                destination: nodes[2],
                mode: TransportMode::Road,
            },
            &mut RoutingWorkspace::new(),
        )
        .unwrap();
    let (new_plan, _) = sim.submit_store_route_plan(route).unwrap();
    assign(&mut sim, id, new_plan, 30);
    sim.tick().unwrap();
    assert_eq!(
        vehicle(&sim, id).movement_state,
        VehicleMovementState::Arrived
    );
}

#[test]
fn current_edge_can_finish_after_disable_or_removal_without_teleport() {
    for remove in [false, true] {
        let (mut sim, nodes, edges, plan_id) = setup(1, 1);
        let id = all(&sim)[0].id;
        assign(&mut sim, id, plan_id, 3);
        sim.tick().unwrap();
        let command = if remove {
            SimulationCommand::RemoveTransportEdge { edge_id: edges[0] }
        } else {
            SimulationCommand::SetTransportEdgeEnabled {
                edge_id: edges[0],
                enabled: false,
            }
        };
        sim.submit_command(command).unwrap();
        sim.tick().unwrap();
        assert_eq!(vehicle(&sim, id).x, 60);
        assert_eq!(
            vehicle(&sim, id).movement_state,
            VehicleMovementState::FollowingRoute
        );
        for _ in 0..7 {
            sim.tick().unwrap();
        }
        assert_eq!(
            vehicle(&sim, id).movement_state,
            VehicleMovementState::Arrived
        );
        assert_eq!(vehicle(&sim, id).current_node, Some(nodes[2]));
    }
}

#[test]
fn unrelated_revision_and_routing_cost_changes_do_not_invalidate_or_change_speed() {
    let (mut sim, _, edges, plan_id) = setup(1, 1);
    let id = all(&sim)[0].id;
    assign(&mut sim, id, plan_id, 3);
    sim.tick().unwrap();
    let before = plan(&sim, plan_id);
    let (x, _) = sim
        .submit_create_transport_node(TransportPosition {
            x: 999,
            y: 999,
            z: 0,
        })
        .unwrap();
    let (y, _) = sim
        .submit_create_transport_node(TransportPosition {
            x: 1000,
            y: 999,
            z: 0,
        })
        .unwrap();
    sim.submit_create_transport_edge(x, y, TransportMode::Rail, 1, true)
        .unwrap();
    sim.submit_command(SimulationCommand::SetTransportEdgeCost {
        edge_id: edges[0],
        base_cost: 1,
    })
    .unwrap();
    sim.tick().unwrap();
    assert_eq!(vehicle(&sim, id).edge_progress, 60);
    assert_eq!(
        vehicle(&sim, id).movement_state,
        VehicleMovementState::FollowingRoute
    );
    assert_eq!(plan(&sim, plan_id), before);
}

#[test]
fn empty_route_arrives_on_assignment_and_speed_zero_stays_in_place() {
    let (mut sim, nodes, _, plan_id) = setup(1, 2);
    let ids: Vec<_> = all(&sim).iter().map(|v| v.id).collect();
    let route = sim
        .find_route(
            RouteRequest {
                origin: nodes[0],
                destination: nodes[0],
                mode: TransportMode::Road,
            },
            &mut RoutingWorkspace::new(),
        )
        .unwrap();
    let (empty, _) = sim.submit_store_route_plan(route).unwrap();
    assign(&mut sim, ids[0], empty, 99);
    assign(&mut sim, ids[1], plan_id, 0);
    sim.tick().unwrap();
    assert_eq!(
        vehicle(&sim, ids[0]).movement_state,
        VehicleMovementState::Arrived
    );
    assert_eq!(vehicle(&sim, ids[0]).current_node, Some(nodes[0]));
    let stopped = vehicle(&sim, ids[1]);
    assert_eq!(stopped.edge_progress, 0);
    assert_eq!((stopped.x, stopped.y, stopped.z), (0, 0, 0));
    sim.tick().unwrap();
    assert_eq!(vehicle(&sim, ids[1]), stopped);
    assert!(sim.movement_events().is_empty());
}

#[test]
fn invalid_assignment_speed_and_plan_are_explicit_and_preserve_execution() {
    let (mut sim, nodes, _, plan_id) = setup(1, 1);
    let id = all(&sim)[0].id;
    assign(&mut sim, id, plan_id, 1);
    sim.tick().unwrap();
    sim.submit_command(SimulationCommand::AssignVehicleRoute {
        vehicle_id: id,
        route_plan_id: plan_id,
    })
    .unwrap();
    sim.submit_command(SimulationCommand::SetVehicleNetworkSpeed {
        vehicle_id: id,
        speed_units_per_ms: u64::MAX,
    })
    .unwrap();
    sim.submit_command(SimulationCommand::AssignVehicleRoute {
        vehicle_id: VehicleId::from_raw(u64::MAX),
        route_plan_id: plan_id,
    })
    .unwrap();
    sim.submit_command(SimulationCommand::AssignVehicleRoute {
        vehicle_id: id,
        route_plan_id: RoutePlanId::from_raw(u64::MAX),
    })
    .unwrap();
    sim.tick().unwrap();
    assert_eq!(
        sim.command_results()[0].result,
        Err(CommandError::InvalidRouteAssignment)
    );
    assert_eq!(
        sim.command_results()[1].result,
        Err(CommandError::InvalidNetworkSpeed)
    );
    assert!(matches!(
        sim.command_results()[2].result,
        Err(CommandError::InvalidVehicle(_))
    ));
    assert!(matches!(
        sim.command_results()[3].result,
        Err(CommandError::InvalidRoutePlan(_))
    ));
    assert_eq!(vehicle(&sim, id).edge_progress, 20);
    assert_eq!(vehicle(&sim, id).speed_units_per_ms, 1);
    let invalid = Route {
        network_revision: 0,
        origin: nodes[0],
        destination: nodes[2],
        mode: TransportMode::Rail,
        total_cost: 0,
        edges: Vec::new(),
    };
    sim.submit_store_route_plan(invalid).unwrap();
    sim.tick().unwrap();
    assert_eq!(
        sim.command_results()[0].result,
        Err(CommandError::InvalidRoute)
    );
}

#[test]
fn movement_checksum_and_owned_snapshots_include_new_state() {
    let (mut sim, _, _, plan_id) = setup(1, 1);
    let id = all(&sim)[0].id;
    let before = sim.checksum();
    sim.submit_command(SimulationCommand::SetVehicleNetworkSpeed {
        vehicle_id: id,
        speed_units_per_ms: 2,
    })
    .unwrap();
    assert_ne!(sim.checksum(), before);
    assign(&mut sim, id, plan_id, 2);
    sim.tick().unwrap();
    let old = vehicle(&sim, id);
    let copied = old;
    sim.tick().unwrap();
    assert_eq!(old, copied);
    assert_ne!(vehicle(&sim, id), old);
    assert!(sim.route_execution_memory_estimate().movement_arrays_bytes > 0);
    assert!(sim.route_execution_memory_estimate().route_plan_edges_bytes > 0);
}

#[test]
fn routed_movement_snapshots_events_and_checksum_match_across_thread_counts() {
    let scenario = |threads| {
        let (mut sim, _, edges, plan_id) = setup(threads, 4096);
        for (index, vehicle) in all(&sim).iter().enumerate() {
            assign(&mut sim, vehicle.id, plan_id, (index % 6 + 1) as u64);
        }
        let mut events = Vec::new();
        let mut results = Vec::new();
        for tick in 0..25 {
            if tick == 2 {
                sim.submit_command(SimulationCommand::SetTransportEdgeEnabled {
                    edge_id: edges[1],
                    enabled: false,
                })
                .unwrap();
            }
            if tick == 3 {
                sim.submit_command(SimulationCommand::RemoveTransportEdge { edge_id: edges[0] })
                    .unwrap();
            }
            sim.tick().unwrap();
            events.push(sim.movement_events().to_vec());
            results.extend_from_slice(sim.command_results());
        }
        (sim.checksum(), all(&sim), events, results)
    };
    let reference = scenario(1);
    for threads in [2, 4, 8] {
        assert_eq!(scenario(threads), reference);
    }
}
