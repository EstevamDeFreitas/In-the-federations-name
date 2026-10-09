use federation_sim::*;

fn simulation(threads: usize) -> Simulation {
    Simulation::new(SimulationConfig {
        width: 65,
        height: 35,
        chunk_size: 32,
        vehicles: 4096,
        threads,
        seed: 42,
        tick_ms: 20,
    })
    .unwrap()
}
fn height(x: u32, y: u32, height: i32) -> SimulationCommand {
    SimulationCommand::SetTerrainVertexHeight { x, y, height }
}
fn all(sim: &Simulation) -> SimulationSnapshot {
    sim.snapshot(SnapshotRequest {
        vehicles: VehicleSelection::All,
        ..Default::default()
    })
    .unwrap()
}

#[test]
fn submission_is_deferred_ordered_and_results_are_per_tick() {
    let mut sim = simulation(1);
    let before = sim.terrain().heights().to_vec();
    let a = sim.submit_command(height(20, 20, 4)).unwrap();
    let b = sim.submit_command(height(20, 20, 7)).unwrap();
    assert!(a < b);
    assert_eq!(sim.terrain().heights(), before);
    assert!(sim.command_results().is_empty());
    sim.tick().unwrap();
    assert_eq!(sim.terrain().height_at(20, 20).unwrap(), 7);
    assert_eq!(
        sim.command_results(),
        [
            CommandResult {
                sequence: a,
                result: Ok(())
            },
            CommandResult {
                sequence: b,
                result: Ok(())
            }
        ]
    );
    assert!(sim.submit_command(height(0, 0, 1)).unwrap() > b);
    sim.tick().unwrap();
    sim.tick().unwrap();
    assert!(sim.command_results().is_empty());
}

#[test]
fn invalid_commands_preserve_state_and_allow_following_commands() {
    let mut sim = simulation(2);
    let id = sim.vehicles().ids()[0];
    let missing = VehicleId::from_raw(u64::MAX);
    sim.submit_command(height(66, 0, 3)).unwrap();
    sim.submit_command(SimulationCommand::SetVehicleVelocity {
        vehicle_id: missing,
        vx: 1,
        vy: 2,
    })
    .unwrap();
    sim.submit_command(SimulationCommand::SetVehicleVelocity {
        vehicle_id: id,
        vx: i64::MAX,
        vy: i64::MIN,
    })
    .unwrap();
    sim.submit_command(height(0, 0, i32::MIN)).unwrap();
    let velocity = sim.vehicles().get(id).unwrap();
    sim.tick().unwrap();
    let results = sim.command_results();
    assert_eq!(
        results[0].result,
        Err(CommandError::InvalidVertex { x: 66, y: 0 })
    );
    assert_eq!(
        results[1].result,
        Err(CommandError::InvalidVehicle(missing))
    );
    assert_eq!(results[2].result, Err(CommandError::InvalidVelocity));
    assert_eq!(results[3].result, Ok(()));
    assert_eq!(sim.vehicles().get(id).unwrap().vx, velocity.vx);
    assert_eq!(sim.vehicles().get(id).unwrap().vy, velocity.vy);
    assert_eq!(sim.terrain().height_at(0, 0).unwrap(), i32::MIN);
}

#[test]
fn velocity_applies_before_movement_and_identity_is_stable() {
    let mut sim = simulation(4);
    let id = all(&sim).vehicles[12].id;
    let before = sim.vehicles().get(id).unwrap();
    sim.submit_command(SimulationCommand::SetVehicleVelocity {
        vehicle_id: id,
        vx: 5,
        vy: -9,
    })
    .unwrap();
    assert_eq!(sim.vehicles().get(id).unwrap(), before);
    sim.tick().unwrap();
    let after = sim.vehicles().get(id).unwrap();
    assert_eq!(after.id, id);
    assert_eq!((after.vx, after.vy), (5, -9));
    assert_eq!(after.x, (before.x + 100).rem_euclid(65 * 1024));
    assert_eq!(after.y, (before.y - 180).rem_euclid(35 * 1024));
    for _ in 0..10 {
        sim.tick().unwrap();
    }
    assert_eq!(sim.vehicles().get(id).unwrap().id, id);
}

#[test]
fn snapshots_own_selected_data_across_ticks_and_edits() {
    let mut sim = simulation(1);
    sim.tick().unwrap();
    let id = sim.vehicles().ids()[0];
    let ids = [id];
    let chunks = [ChunkRevisionQuery {
        x: 2,
        y: 1,
        known_revision: None,
    }];
    let request = SnapshotRequest {
        vehicles: VehicleSelection::Ids(&ids),
        terrain_chunks: &chunks,
    };
    let old = sim.snapshot(request).unwrap();
    let saved = old.clone();
    assert_eq!(old.tick, 1);
    assert_eq!(old.vehicles, [sim.vehicles().get(id).unwrap()]);
    let chunk = &old.terrain_chunks[0];
    assert_eq!(
        (
            chunk.origin_x,
            chunk.origin_y,
            chunk.vertices_x,
            chunk.vertices_y
        ),
        (64, 32, 2, 4)
    );
    assert_eq!(chunk.heights.len(), 8);
    sim.submit_command(height(65, 35, 11)).unwrap();
    for _ in 0..3 {
        sim.tick().unwrap();
    }
    assert_eq!(old, saved);
    assert_ne!(sim.snapshot(request).unwrap(), old);
    assert_eq!(
        sim.snapshot(SnapshotRequest::default())
            .unwrap()
            .vehicles
            .len(),
        0
    );
}

#[test]
fn revisions_support_shared_vertices_and_independent_consumers() {
    let mut sim = simulation(1);
    let queries: Vec<_> = sim
        .terrain()
        .chunks()
        .iter()
        .map(|c| ChunkRevisionQuery {
            x: c.x,
            y: c.y,
            known_revision: Some(c.revision),
        })
        .collect();
    let request = SnapshotRequest {
        terrain_chunks: &queries,
        ..Default::default()
    };
    assert!(sim.snapshot(request).unwrap().terrain_chunks.is_empty());
    sim.submit_command(height(32, 32, 1)).unwrap();
    sim.tick().unwrap();
    let changed = sim.snapshot(request).unwrap().terrain_chunks;
    assert_eq!(
        changed
            .iter()
            .map(|c| (c.x, c.y, c.revision))
            .collect::<Vec<_>>(),
        [(0, 0, 1), (1, 0, 1), (0, 1, 1), (1, 1, 1)]
    );
    let current: Vec<_> = changed
        .iter()
        .map(|c| ChunkRevisionQuery {
            x: c.x,
            y: c.y,
            known_revision: Some(c.revision),
        })
        .collect();
    assert!(
        sim.snapshot(SnapshotRequest {
            terrain_chunks: &current,
            ..Default::default()
        })
        .unwrap()
        .terrain_chunks
        .is_empty()
    );
    assert_eq!(sim.snapshot(request).unwrap().terrain_chunks, changed);
    sim.submit_command(height(32, 32, 1)).unwrap();
    sim.tick().unwrap();
    assert_eq!(sim.snapshot(request).unwrap().terrain_chunks, changed);
}

#[test]
fn invalid_snapshot_requests_return_errors() {
    let sim = simulation(1);
    let ids = [VehicleId::from_raw(u64::MAX)];
    assert!(matches!(
        sim.snapshot(SnapshotRequest {
            vehicles: VehicleSelection::Ids(&ids),
            ..Default::default()
        }),
        Err(SimulationError::InvalidVehicle(_))
    ));
    let chunks = [ChunkRevisionQuery {
        x: 3,
        y: 0,
        known_revision: None,
    }];
    assert!(matches!(
        sim.snapshot(SnapshotRequest {
            terrain_chunks: &chunks,
            ..Default::default()
        }),
        Err(SimulationError::OutOfBounds { .. })
    ));
}

#[test]
fn pending_commands_and_sequence_progress_are_deterministic_state() {
    let mut a = simulation(1);
    let mut b = simulation(1);
    assert_eq!(a.checksum(), b.checksum());
    a.submit_command(height(0, 0, 0)).unwrap();
    assert_ne!(a.checksum(), b.checksum());
    a.tick().unwrap();
    b.tick().unwrap();
    assert_eq!(a.terrain().heights(), b.terrain().heights());
    assert_ne!(a.checksum(), b.checksum());
    b.submit_command(height(0, 0, 0)).unwrap();
    a.tick().unwrap();
    b.tick().unwrap();
    assert_eq!(a.checksum(), b.checksum());
    a.submit_command(height(0, 0, 1)).unwrap();
    b.submit_command(height(0, 0, 2)).unwrap();
    assert_ne!(a.checksum(), b.checksum());
}

#[test]
fn commands_results_and_snapshots_match_across_thread_counts() {
    let scenario = |threads| {
        let mut sim = simulation(threads);
        let id = sim.vehicles().ids()[23];
        let mut results = Vec::new();
        for tick in 0..20 {
            sim.submit_command(height(32, 32, tick)).unwrap();
            sim.submit_command(height(32, 32, tick + 1)).unwrap();
            sim.submit_command(height(99, 99, 0)).unwrap();
            sim.submit_command(SimulationCommand::SetVehicleVelocity {
                vehicle_id: id,
                vx: i64::from(tick) - 10,
                vy: 7,
            })
            .unwrap();
            sim.tick().unwrap();
            results.extend_from_slice(sim.command_results());
        }
        let chunks: Vec<_> = sim
            .terrain()
            .chunks()
            .iter()
            .map(|c| ChunkRevisionQuery {
                x: c.x,
                y: c.y,
                known_revision: None,
            })
            .collect();
        (
            sim.checksum(),
            sim.snapshot(SnapshotRequest {
                vehicles: VehicleSelection::All,
                terrain_chunks: &chunks,
            })
            .unwrap(),
            results,
        )
    };
    let reference = scenario(1);
    for threads in [2, 4, 8] {
        assert_eq!(scenario(threads), reference);
    }
}
