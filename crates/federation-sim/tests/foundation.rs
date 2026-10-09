use federation_sim::*;
fn config(threads: usize) -> SimulationConfig {
    SimulationConfig {
        width: 65,
        height: 35,
        chunk_size: 32,
        vehicles: 4096,
        threads,
        seed: 42,
        tick_ms: 20,
    }
}
fn snapshot(sim: &Simulation) -> SimulationSnapshot {
    let queries: Vec<_> = (0..2)
        .flat_map(|y| {
            (0..3).map(move |x| ChunkRevisionQuery {
                x,
                y,
                known_revision: None,
            })
        })
        .collect();
    sim.snapshot(SnapshotRequest {
        vehicles: VehicleSelection::All,
        terrain_chunks: &queries,
        ..Default::default()
    })
    .unwrap()
}
#[test]
fn valid_and_invalid_configuration() {
    assert!(Simulation::new(config(1)).is_ok());
    for field in 0..8 {
        let mut c = config(1);
        match field {
            0 => c.width = 0,
            1 => c.height = 0,
            2 => c.chunk_size = 0,
            3 => c.threads = 0,
            4 => c.tick_ms = 0,
            5 => c.width = u32::MAX,
            6 => c.vehicles = 10_000_001,
            _ => c.threads = 1025,
        }
        assert!(Simulation::new(c).is_err());
    }
}
#[test]
fn snapshot_metadata_and_partial_chunk_bounds() {
    let sim = Simulation::new(config(1)).unwrap();
    let snap = snapshot(&sim);
    assert_eq!(
        snap.region,
        RegionSnapshotMetadata {
            width: 65,
            height: 35,
            chunk_size: 32
        }
    );
    assert_eq!(snap.terrain_chunks.len(), 6);
    assert_eq!(
        (
            snap.terrain_chunks[5].vertices_x,
            snap.terrain_chunks[5].vertices_y
        ),
        (2, 4)
    );
    assert!(
        sim.snapshot(SnapshotRequest {
            terrain_chunks: &[ChunkRevisionQuery {
                x: 3,
                y: 0,
                known_revision: None
            }],
            ..Default::default()
        })
        .is_err()
    );
}
#[test]
fn stable_ids_and_correct_movement_without_storage_access() {
    let mut sim = Simulation::new(config(2)).unwrap();
    let before = snapshot(&sim);
    assert_eq!(
        before
            .vehicles
            .iter()
            .map(|v| v.id)
            .collect::<std::collections::HashSet<_>>()
            .len(),
        4096
    );
    sim.tick().unwrap();
    let after = snapshot(&sim);
    for (a, b) in before.vehicles.iter().zip(&after.vehicles) {
        assert_eq!(a.id, b.id);
        assert_eq!(b.x, (a.x + a.vx * 20).rem_euclid(65 * 1024));
        assert_eq!(b.y, (a.y + a.vy * 20).rem_euclid(35 * 1024));
    }
    for _ in 0..100 {
        sim.tick().unwrap();
    }
    let after = snapshot(&sim);
    assert_eq!(after.tick, 101);
    for (a, b) in before.vehicles.iter().zip(&after.vehicles) {
        assert_eq!(a.id, b.id);
        assert!((0..65 * 1024).contains(&b.x));
        assert!((0..35 * 1024).contains(&b.y));
    }
}
#[test]
fn same_seed_and_different_thread_counts_match_full_state() {
    let mut reference = Simulation::new(config(1)).unwrap();
    for _ in 0..30 {
        reference.tick().unwrap();
    }
    for threads in [1, 2, 4, 8] {
        let mut sim = Simulation::new(config(threads)).unwrap();
        for _ in 0..30 {
            sim.tick().unwrap();
        }
        assert_eq!(sim.checksum(), reference.checksum());
        assert_eq!(snapshot(&sim), snapshot(&reference));
    }
    let mut other = config(1);
    other.seed = 43;
    assert_ne!(
        snapshot(&Simulation::new(other).unwrap()).vehicles,
        snapshot(&Simulation::new(config(1)).unwrap()).vehicles
    );
}
#[test]
fn empty_simulation_and_on_demand_diagnostics() {
    let mut c = config(1);
    c.vehicles = 0;
    let mut sim = Simulation::new(c).unwrap();
    assert!(snapshot(&sim).vehicles.is_empty());
    let initial = sim.checksum();
    for _ in 0..5 {
        sim.tick().unwrap();
    }
    let metrics = sim.diagnostics();
    assert_eq!(metrics.tick_count, 5);
    assert_eq!(metrics.logical_time_ms, 100);
    assert_eq!(metrics.vehicles, 0);
    assert_ne!(metrics.checksum, initial);
    let before = sim.checksum();
    sim.submit_command(SimulationCommand::SetTerrainVertexHeight {
        x: 32,
        y: 32,
        height: 5,
    })
    .unwrap();
    assert_ne!(before, sim.checksum());
}
#[test]
fn supports_one_hundred_thousand_vehicles() {
    let mut c = config(4);
    c.vehicles = 100_000;
    let mut sim = Simulation::new(c).unwrap();
    sim.tick().unwrap();
    assert_eq!(sim.diagnostics().vehicles, 100_000);
    assert_eq!(snapshot(&sim).vehicles.len(), 100_000);
}
