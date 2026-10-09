use federation_sim::*;

#[test]
fn specialized_submission_allocates_ids_in_command_call_order() {
    let mut sim = simulation(1);
    let (a, seq_a) = sim
        .submit_create_transport_node(TransportPosition::default())
        .unwrap();
    let (b, seq_b) = sim
        .submit_create_transport_node(TransportPosition::default())
        .unwrap();
    let (edge, seq_edge) = sim
        .submit_create_transport_edge(a, b, TransportMode::Road, 2, true)
        .unwrap();
    assert!(a < b);
    assert!(seq_a < seq_b && seq_b < seq_edge);
    assert!(network(&sim).nodes.is_empty());
    sim.tick().unwrap();
    assert!(sim.command_results().iter().all(|r| r.result.is_ok()));
    assert_eq!(
        route(&sim, a, b, TransportMode::Road).unwrap().edges,
        [edge]
    );
}

#[test]
fn dijkstra_costs_match_floyd_warshall_on_seeded_directed_multigraphs() {
    const N: usize = 8;
    for seed in [42, 99, 1234] {
        let mut sim = simulation(1);
        let nodes: Vec<_> = (0..N)
            .map(|_| node(&mut sim, TransportPosition::default()))
            .collect();
        let mut rng = federation_sim::rng::DeterministicRng::new(seed);
        for &from in &nodes {
            for &to in &nodes {
                if from == to {
                    continue;
                }
                for _ in 0..2 {
                    if rng.next_u64() % 3 == 0 {
                        continue;
                    }
                    let mode = if rng.next_u64() % 2 == 0 {
                        TransportMode::Road
                    } else {
                        TransportMode::Rail
                    };
                    let cost = if rng.next_u64() % 17 == 0 {
                        u64::MAX
                    } else {
                        rng.next_u64() % 20 + 1
                    };
                    let id = edge(&mut sim, from, to, cost, mode);
                    if rng.next_u64() % 4 == 0 {
                        sim.submit_command(SimulationCommand::SetTransportEdgeEnabled {
                            edge_id: id,
                            enabled: false,
                        })
                        .unwrap();
                    }
                }
            }
        }
        sim.tick().unwrap();
        let snapshot = network(&sim);
        for mode in [TransportMode::Road, TransportMode::Rail] {
            let mut distances = [[u128::MAX; N]; N];
            for (i, row) in distances.iter_mut().enumerate() {
                row[i] = 0;
            }
            for edge in snapshot
                .edges
                .iter()
                .filter(|e| e.enabled && e.mode == mode)
            {
                let from = nodes.iter().position(|id| *id == edge.from).unwrap();
                let to = nodes.iter().position(|id| *id == edge.to).unwrap();
                distances[from][to] = distances[from][to].min(u128::from(edge.base_cost));
            }
            for k in 0..N {
                for i in 0..N {
                    for j in 0..N {
                        if distances[i][k] != u128::MAX && distances[k][j] != u128::MAX {
                            distances[i][j] =
                                distances[i][j].min(distances[i][k] + distances[k][j]);
                        }
                    }
                }
            }
            let mut workspace = RoutingWorkspace::new();
            for (i, &origin) in nodes.iter().enumerate() {
                for (j, &destination) in nodes.iter().enumerate() {
                    let result = sim.find_route(
                        RouteRequest {
                            origin,
                            destination,
                            mode,
                        },
                        &mut workspace,
                    );
                    if distances[i][j] == u128::MAX {
                        assert_eq!(result, Err(RouteError::NoRoute));
                        continue;
                    }
                    let route = result.unwrap();
                    assert_eq!(route.total_cost, distances[i][j]);
                    let mut current = origin;
                    let mut total = 0_u128;
                    for id in route.edges {
                        let edge = snapshot.edges.iter().find(|e| e.id == id).unwrap();
                        assert_eq!(edge.from, current);
                        assert!(edge.enabled);
                        assert_eq!(edge.mode, mode);
                        total += u128::from(edge.base_cost);
                        current = edge.to;
                    }
                    assert_eq!(current, destination);
                    assert_eq!(total, distances[i][j]);
                }
            }
        }
    }
}

fn simulation(threads: usize) -> Simulation {
    Simulation::new(SimulationConfig {
        width: 8,
        height: 8,
        chunk_size: 4,
        vehicles: 2048,
        threads,
        seed: 42,
        ..Default::default()
    })
    .unwrap()
}
fn node(sim: &mut Simulation, position: TransportPosition) -> TransportNodeId {
    let id = sim.reserve_transport_node_id().unwrap();
    sim.submit_command(SimulationCommand::CreateTransportNode {
        node_id: id,
        position,
    })
    .unwrap();
    id
}
fn edge(
    sim: &mut Simulation,
    from: TransportNodeId,
    to: TransportNodeId,
    cost: u64,
    mode: TransportMode,
) -> TransportEdgeId {
    let id = sim.reserve_transport_edge_id().unwrap();
    sim.submit_command(SimulationCommand::CreateTransportEdge {
        edge_id: id,
        from,
        to,
        mode,
        base_cost: cost,
        enabled: true,
    })
    .unwrap();
    id
}
fn network(sim: &Simulation) -> TransportSnapshot {
    sim.snapshot(SnapshotRequest {
        transport: Some(TransportSnapshotRequest {
            include_nodes: true,
            include_edges: true,
            known_revision: None,
        }),
        ..Default::default()
    })
    .unwrap()
    .transport
    .unwrap()
}
fn route(
    sim: &Simulation,
    from: TransportNodeId,
    to: TransportNodeId,
    mode: TransportMode,
) -> Result<Route, RouteError> {
    sim.find_route(
        RouteRequest {
            origin: from,
            destination: to,
            mode,
        },
        &mut RoutingWorkspace::new(),
    )
}

#[test]
fn dependent_creation_is_deferred_and_builds_directed_multigraph() {
    let mut sim = simulation(1);
    let position = TransportPosition {
        x: 1024,
        y: 2048,
        z: 7,
    };
    let a = node(&mut sim, position);
    let b = node(&mut sim, position); // Same coordinates, different identities.
    let road = edge(&mut sim, a, b, 5, TransportMode::Road);
    let parallel = edge(&mut sim, a, b, 3, TransportMode::Road);
    let rail = edge(&mut sim, a, b, 2, TransportMode::Rail);
    let back = edge(&mut sim, b, a, 9, TransportMode::Road);
    assert!(network(&sim).nodes.is_empty());
    assert!(network(&sim).edges.is_empty());
    sim.tick().unwrap();
    assert!(sim.command_results().iter().all(|r| r.result.is_ok()));
    let snapshot = network(&sim);
    assert_eq!(snapshot.nodes.len(), 2);
    assert_eq!(snapshot.edges.len(), 4);
    assert_eq!(snapshot.network_revision, 6);
    assert_eq!(snapshot.nodes[0].position, position);
    assert_eq!(
        route(&sim, a, b, TransportMode::Road).unwrap().edges,
        [parallel]
    );
    assert_eq!(
        route(&sim, a, b, TransportMode::Rail).unwrap().edges,
        [rail]
    );
    assert_eq!(
        route(&sim, b, a, TransportMode::Road).unwrap().edges,
        [back]
    );
    assert_eq!(
        route(&sim, b, a, TransportMode::Rail),
        Err(RouteError::NoRoute)
    );
    assert_ne!(road, parallel);
}

#[test]
fn invalid_edges_nodes_and_duplicates_allow_following_commands() {
    let mut sim = simulation(1);
    let a = node(&mut sim, TransportPosition::default());
    let b = node(&mut sim, TransportPosition::default());
    edge(
        &mut sim,
        a,
        TransportNodeId::from_raw(u64::MAX),
        1,
        TransportMode::Road,
    );
    edge(&mut sim, a, b, 0, TransportMode::Road);
    edge(&mut sim, a, a, 1, TransportMode::Road);
    let valid = edge(&mut sim, a, b, 1, TransportMode::Road);
    sim.submit_command(SimulationCommand::CreateTransportEdge {
        edge_id: TransportEdgeId::from_raw(u64::MAX),
        from: a,
        to: b,
        mode: TransportMode::Road,
        base_cost: 1,
        enabled: true,
    })
    .unwrap();
    sim.submit_command(SimulationCommand::CreateTransportNode {
        node_id: TransportNodeId::from_raw(u64::MAX),
        position: TransportPosition::default(),
    })
    .unwrap();
    node(
        &mut sim,
        TransportPosition {
            x: i64::MIN,
            y: 0,
            z: 0,
        },
    );
    sim.submit_command(SimulationCommand::CreateTransportNode {
        node_id: a,
        position: TransportPosition::default(),
    })
    .unwrap();
    sim.submit_command(SimulationCommand::CreateTransportEdge {
        edge_id: valid,
        from: a,
        to: b,
        mode: TransportMode::Rail,
        base_cost: 2,
        enabled: false,
    })
    .unwrap();
    sim.tick().unwrap();
    let results = sim.command_results();
    assert!(matches!(
        results[2].result,
        Err(CommandError::InvalidTransportNode(_))
    ));
    assert_eq!(results[3].result, Err(CommandError::InvalidTransportCost));
    assert_eq!(results[4].result, Err(CommandError::TransportSelfLoop));
    assert_eq!(results[5].result, Ok(()));
    assert!(matches!(
        results[6].result,
        Err(CommandError::InvalidTransportEdge(_))
    ));
    assert!(matches!(
        results[7].result,
        Err(CommandError::InvalidTransportNode(_))
    ));
    assert_eq!(
        results[8].result,
        Err(CommandError::InvalidTransportPosition)
    );
    assert_eq!(results[9].result, Err(CommandError::TransportNodeIdUsed(a)));
    assert_eq!(
        results[10].result,
        Err(CommandError::TransportEdgeIdUsed(valid))
    );
    assert_eq!(network(&sim).network_revision, 3);
}

#[test]
fn removals_check_all_live_incident_edges_and_never_reuse_ids() {
    let mut sim = simulation(1);
    let abandoned = sim.reserve_transport_node_id().unwrap();
    let a = node(&mut sim, TransportPosition::default());
    let b = node(&mut sim, TransportPosition::default());
    let e = edge(&mut sim, a, b, 1, TransportMode::Road);
    sim.tick().unwrap();
    sim.submit_command(SimulationCommand::SetTransportEdgeEnabled {
        edge_id: e,
        enabled: false,
    })
    .unwrap();
    sim.submit_command(SimulationCommand::RemoveTransportNode { node_id: b })
        .unwrap();
    sim.submit_command(SimulationCommand::RemoveTransportNode { node_id: a })
        .unwrap();
    sim.submit_command(SimulationCommand::RemoveTransportEdge { edge_id: e })
        .unwrap();
    sim.submit_command(SimulationCommand::RemoveTransportNode { node_id: b })
        .unwrap();
    sim.submit_command(SimulationCommand::RemoveTransportNode { node_id: a })
        .unwrap();
    sim.tick().unwrap();
    assert_eq!(
        sim.command_results()[1].result,
        Err(CommandError::TransportNodeInUse(b))
    );
    assert_eq!(
        sim.command_results()[2].result,
        Err(CommandError::TransportNodeInUse(a))
    );
    assert!(network(&sim).nodes.is_empty());
    assert!(network(&sim).edges.is_empty());
    let next = node(&mut sim, TransportPosition::default());
    assert!(next > a && next > b && next > abandoned);
    let next_edge = sim.reserve_transport_edge_id().unwrap();
    assert!(next_edge > e);
    sim.submit_command(SimulationCommand::CreateTransportNode {
        node_id: a,
        position: TransportPosition::default(),
    })
    .unwrap();
    sim.submit_command(SimulationCommand::CreateTransportEdge {
        edge_id: e,
        from: next,
        to: next,
        mode: TransportMode::Road,
        base_cost: 1,
        enabled: true,
    })
    .unwrap();
    sim.tick().unwrap();
    assert_eq!(
        sim.command_results()[1].result,
        Err(CommandError::TransportNodeIdUsed(a))
    );
    assert_eq!(
        sim.command_results()[2].result,
        Err(CommandError::TransportEdgeIdUsed(e))
    );
    assert_eq!(
        route(&sim, a, next, TransportMode::Road),
        Err(RouteError::InvalidOrigin(a))
    );
}

#[test]
fn effective_edits_increment_revision_and_noops_or_errors_do_not() {
    let mut sim = simulation(1);
    let a = node(&mut sim, TransportPosition::default());
    sim.tick().unwrap();
    assert_eq!(network(&sim).network_revision, 1);
    let b = node(&mut sim, TransportPosition::default());
    let e = edge(&mut sim, a, b, 5, TransportMode::Road);
    sim.tick().unwrap();
    assert_eq!(network(&sim).network_revision, 3);
    for (command, revision) in [
        (
            SimulationCommand::SetTransportEdgeEnabled {
                edge_id: e,
                enabled: false,
            },
            4,
        ),
        (
            SimulationCommand::SetTransportEdgeEnabled {
                edge_id: e,
                enabled: false,
            },
            4,
        ),
        (
            SimulationCommand::SetTransportEdgeCost {
                edge_id: e,
                base_cost: 5,
            },
            4,
        ),
        (
            SimulationCommand::SetTransportEdgeCost {
                edge_id: e,
                base_cost: 0,
            },
            4,
        ),
        (
            SimulationCommand::SetTransportEdgeCost {
                edge_id: e,
                base_cost: 7,
            },
            5,
        ),
        (
            SimulationCommand::SetTransportEdgeEnabled {
                edge_id: e,
                enabled: true,
            },
            6,
        ),
        (SimulationCommand::RemoveTransportEdge { edge_id: e }, 7),
        (SimulationCommand::RemoveTransportEdge { edge_id: e }, 7),
        (SimulationCommand::RemoveTransportNode { node_id: a }, 8),
        (SimulationCommand::RemoveTransportNode { node_id: a }, 8),
    ] {
        sim.submit_command(command).unwrap();
        sim.tick().unwrap();
        assert_eq!(network(&sim).network_revision, revision);
    }
}

#[test]
fn routing_chooses_minimum_cost_and_changes_after_edits() {
    let mut sim = simulation(1);
    let a = node(&mut sim, TransportPosition::default());
    let b = node(&mut sim, TransportPosition::default());
    let c = node(&mut sim, TransportPosition::default());
    let d = node(&mut sim, TransportPosition::default());
    let ab = edge(&mut sim, a, b, 5, TransportMode::Road);
    let bd = edge(&mut sim, b, d, 5, TransportMode::Road);
    let ac = edge(&mut sim, a, c, 20, TransportMode::Road);
    let cd = edge(&mut sim, c, d, 1, TransportMode::Road);
    edge(&mut sim, a, d, 1, TransportMode::Rail);
    sim.tick().unwrap();
    let first = route(&sim, a, d, TransportMode::Road).unwrap();
    assert_eq!(first.total_cost, 10);
    assert_eq!(first.edges, [ab, bd]);
    assert_eq!(
        route(&sim, d, a, TransportMode::Road),
        Err(RouteError::NoRoute)
    );
    sim.submit_command(SimulationCommand::SetTransportEdgeEnabled {
        edge_id: bd,
        enabled: false,
    })
    .unwrap();
    sim.tick().unwrap();
    let second = route(&sim, a, d, TransportMode::Road).unwrap();
    assert_eq!(second.total_cost, 21);
    assert_eq!(second.edges, [ac, cd]);
    assert!(second.network_revision > first.network_revision);
    sim.submit_command(SimulationCommand::SetTransportEdgeEnabled {
        edge_id: bd,
        enabled: true,
    })
    .unwrap();
    sim.submit_command(SimulationCommand::SetTransportEdgeCost {
        edge_id: ab,
        base_cost: 50,
    })
    .unwrap();
    sim.tick().unwrap();
    assert_eq!(
        route(&sim, a, d, TransportMode::Road).unwrap().edges,
        [ac, cd]
    );
}

#[test]
fn origin_equals_destination_and_invalid_endpoints_are_explicit() {
    let mut sim = simulation(1);
    let a = node(&mut sim, TransportPosition::default());
    sim.tick().unwrap();
    let r = route(&sim, a, a, TransportMode::Rail).unwrap();
    assert_eq!(r.total_cost, 0);
    assert!(r.edges.is_empty());
    let invalid = TransportNodeId::from_raw(u64::MAX);
    assert_eq!(
        route(&sim, invalid, a, TransportMode::Road),
        Err(RouteError::InvalidOrigin(invalid))
    );
    assert_eq!(
        route(&sim, a, invalid, TransportMode::Road),
        Err(RouteError::InvalidDestination(invalid))
    );
}

#[test]
fn maximum_edge_costs_sum_without_overflow() {
    let mut sim = simulation(1);
    let a = node(&mut sim, TransportPosition::default());
    let b = node(&mut sim, TransportPosition::default());
    let c = node(&mut sim, TransportPosition::default());
    let ab = edge(&mut sim, a, b, u64::MAX, TransportMode::Road);
    let bc = edge(&mut sim, b, c, u64::MAX, TransportMode::Road);
    sim.tick().unwrap();
    let result = route(&sim, a, c, TransportMode::Road).unwrap();
    assert_eq!(result.total_cost, u128::from(u64::MAX) * 2);
    assert_eq!(result.edges, [ab, bc]);
}

#[test]
fn equal_cost_ties_use_smallest_predecessor_edge_and_are_repeatable() {
    let mut sim = simulation(1);
    let a = node(&mut sim, TransportPosition::default());
    let b = node(&mut sim, TransportPosition::default());
    let c = node(&mut sim, TransportPosition::default());
    let d = node(&mut sim, TransportPosition::default());
    let cd = edge(&mut sim, c, d, 5, TransportMode::Road);
    let bd = edge(&mut sim, b, d, 5, TransportMode::Road);
    let ab = edge(&mut sim, a, b, 5, TransportMode::Road);
    let ac = edge(&mut sim, a, c, 5, TransportMode::Road);
    sim.tick().unwrap();
    let request = RouteRequest {
        origin: a,
        destination: d,
        mode: TransportMode::Road,
    };
    let mut workspace = RoutingWorkspace::new();
    let checksum = sim.checksum();
    for _ in 0..20 {
        let result = sim.find_route(request, &mut workspace).unwrap();
        assert_eq!(result.edges, [ac, cd]);
        assert_eq!(
            result,
            sim.find_route(request, &mut RoutingWorkspace::new())
                .unwrap()
        );
    }
    assert_eq!(sim.checksum(), checksum);
    assert!(workspace.diagnostics().visited_nodes > 0);
    assert!(workspace.diagnostics().relaxed_edges >= 4);
    assert!(workspace.diagnostics().heap_pushes >= 4);
    assert!(bd > cd && ac > ab);
}

#[test]
fn network_snapshots_are_selective_owned_and_revision_based() {
    let mut sim = simulation(1);
    let a = node(&mut sim, TransportPosition { x: 0, y: 0, z: 5 });
    let b = node(&mut sim, TransportPosition { x: 0, y: 0, z: -5 });
    let e = edge(&mut sim, a, b, 1, TransportMode::Road);
    sim.tick().unwrap();
    let old = network(&sim);
    let copy = old.clone();
    assert!(
        sim.snapshot(SnapshotRequest::default())
            .unwrap()
            .transport
            .is_none()
    );
    let query = |revision| SnapshotRequest {
        transport: Some(TransportSnapshotRequest {
            include_nodes: true,
            include_edges: true,
            known_revision: Some(revision),
        }),
        ..Default::default()
    };
    let unchanged = sim
        .snapshot(query(old.network_revision))
        .unwrap()
        .transport
        .unwrap();
    assert!(unchanged.unchanged);
    assert!(unchanged.nodes.is_empty());
    assert!(unchanged.edges.is_empty());
    let nodes = sim
        .snapshot(SnapshotRequest {
            transport: Some(TransportSnapshotRequest {
                include_nodes: true,
                ..Default::default()
            }),
            ..Default::default()
        })
        .unwrap()
        .transport
        .unwrap();
    assert_eq!(nodes.nodes, old.nodes);
    assert!(nodes.edges.is_empty());
    sim.submit_command(SimulationCommand::RemoveTransportEdge { edge_id: e })
        .unwrap();
    sim.tick().unwrap();
    let changed = sim
        .snapshot(query(old.network_revision))
        .unwrap()
        .transport
        .unwrap();
    assert!(!changed.unchanged);
    assert!(changed.edges.is_empty());
    assert_eq!(old, copy);
    assert_eq!(old.edges[0].id, e);
}

#[test]
fn workspace_handles_network_growth_removal_and_unreachable_queries() {
    let mut sim = simulation(1);
    let a = node(&mut sim, TransportPosition::default());
    let b = node(&mut sim, TransportPosition::default());
    sim.tick().unwrap();
    let request = RouteRequest {
        origin: a,
        destination: b,
        mode: TransportMode::Road,
    };
    let mut workspace = RoutingWorkspace::new();
    assert_eq!(
        sim.find_route(request, &mut workspace),
        Err(RouteError::NoRoute)
    );
    let c = node(&mut sim, TransportPosition::default());
    edge(&mut sim, a, c, 1, TransportMode::Road);
    let cb = edge(&mut sim, c, b, 1, TransportMode::Road);
    sim.tick().unwrap();
    assert_eq!(
        sim.find_route(request, &mut workspace).unwrap().total_cost,
        2
    );
    sim.submit_command(SimulationCommand::RemoveTransportEdge { edge_id: cb })
        .unwrap();
    sim.tick().unwrap();
    assert_eq!(
        sim.find_route(request, &mut workspace),
        Err(RouteError::NoRoute)
    );
}

#[test]
fn reservation_and_pending_transport_commands_change_checksum() {
    let mut sim = simulation(1);
    let initial = sim.checksum();
    let a = sim.reserve_transport_node_id().unwrap();
    assert_ne!(sim.checksum(), initial);
    let reserved = sim.checksum();
    sim.submit_command(SimulationCommand::CreateTransportNode {
        node_id: a,
        position: TransportPosition::default(),
    })
    .unwrap();
    assert_ne!(sim.checksum(), reserved);
    let queued = sim.checksum();
    sim.reserve_transport_edge_id().unwrap();
    assert_ne!(sim.checksum(), queued);
    assert_eq!(network(&sim).network_revision, 0);
}

#[test]
fn full_scenario_matches_across_one_two_four_and_eight_threads() {
    let scenario = |threads| {
        let mut sim = simulation(threads);
        let a = node(&mut sim, TransportPosition::default());
        let b = node(&mut sim, TransportPosition::default());
        let e = edge(&mut sim, a, b, 5, TransportMode::Road);
        edge(&mut sim, a, b, 7, TransportMode::Rail);
        let id = sim
            .snapshot(SnapshotRequest {
                vehicles: VehicleSelection::All,
                ..Default::default()
            })
            .unwrap()
            .vehicles[0]
            .id;
        let mut results = Vec::new();
        let mut routes = Vec::new();
        let mut workspace = RoutingWorkspace::new();
        for tick in 0..10 {
            sim.submit_command(SimulationCommand::SetTerrainVertexHeight {
                x: 4,
                y: 4,
                height: tick,
            })
            .unwrap();
            sim.submit_command(SimulationCommand::SetVehicleVelocity {
                vehicle_id: id,
                vx: i64::from(tick),
                vy: -2,
            })
            .unwrap();
            sim.submit_command(SimulationCommand::SetTransportEdgeCost {
                edge_id: e,
                base_cost: tick as u64 + 1,
            })
            .unwrap();
            sim.tick().unwrap();
            results.extend_from_slice(sim.command_results());
            routes.push(
                sim.find_route(
                    RouteRequest {
                        origin: a,
                        destination: b,
                        mode: TransportMode::Road,
                    },
                    &mut workspace,
                )
                .unwrap(),
            );
        }
        let queries = [ChunkRevisionQuery {
            x: 0,
            y: 0,
            known_revision: None,
        }];
        let snapshot = sim
            .snapshot(SnapshotRequest {
                vehicles: VehicleSelection::All,
                terrain_chunks: &queries,
                transport: Some(TransportSnapshotRequest {
                    include_nodes: true,
                    include_edges: true,
                    known_revision: None,
                }),
            })
            .unwrap();
        (sim.checksum(), snapshot, routes, results)
    };
    let reference = scenario(1);
    for threads in [2, 4, 8] {
        assert_eq!(scenario(threads), reference);
    }
}
