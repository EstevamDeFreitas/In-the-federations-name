use federation_sim::{Simulation, SimulationConfig, terrain::Terrain};

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

#[test]
fn valid_and_invalid_configuration() {
    assert!(Simulation::new(config(1)).is_ok());
    for field in 0..7 {
        let mut c = config(1);
        match field {
            0 => c.width = 0,
            1 => c.height = 0,
            2 => c.chunk_size = 0,
            3 => c.threads = 0,
            4 => c.tick_ms = 0,
            5 => c.width = u32::MAX,
            _ => c.vehicles = 10_000_001,
        }
        assert!(Simulation::new(c).is_err());
    }
    assert!(Terrain::new(1, 1, 0).is_err());
    assert!(Terrain::new(u32::MAX, u32::MAX, 32).is_err());
    let mut c = config(1);
    c.threads = 1025;
    assert!(Simulation::new(c).is_err());
}

#[test]
fn rectangular_map_indexing_and_bounds() {
    let t = Terrain::new(65, 35, 32).unwrap();
    assert_eq!(t.chunk_dimensions(), (3, 2));
    assert_eq!(t.tile_index(64, 34).unwrap(), 65 * 35 - 1);
    assert_eq!(t.vertex_index(65, 35).unwrap(), 66 * 36 - 1);
    assert_eq!(t.chunk_index(2, 1).unwrap(), 5);
    assert!(t.tile_index(65, 0).is_err());
    assert!(t.tile_index(0, 35).is_err());
    assert!(t.vertex_index(66, 0).is_err());
    assert!(t.vertex_index(0, 36).is_err());
    assert!(t.chunk_index(3, 0).is_err());
    assert!(t.chunk_index(0, 2).is_err());
}

#[test]
fn shared_vertices_invalidate_exactly_adjacent_chunks() {
    let cases = [
        ((10, 10), vec![0]),
        ((32, 10), vec![0, 1]),
        ((10, 32), vec![0, 3]),
        ((32, 32), vec![0, 1, 3, 4]),
        ((64, 32), vec![1, 2, 4, 5]),
        ((0, 0), vec![0]),
        ((65, 35), vec![5]),
        ((0, 32), vec![0, 3]),
        ((65, 32), vec![2, 5]),
    ];
    for ((x, y), expected) in cases {
        let mut t = Terrain::new(65, 35, 32).unwrap();
        t.clear_dirty();
        t.set_height(x, y, -7).unwrap();
        assert_eq!(t.height_at(x, y).unwrap(), -7);
        let dirty: Vec<_> = t
            .chunks()
            .iter()
            .enumerate()
            .filter(|(_, c)| c.dirty)
            .map(|(i, _)| i)
            .collect();
        assert_eq!(dirty, expected, "vertex ({x},{y})");
        for c in t.chunks() {
            assert_eq!(c.revision, u64::from(c.dirty));
        }
        t.set_height(x, y, -7).unwrap();
        assert!(t.chunks().iter().all(|c| c.revision <= 1));
        t.set_height(x, y, 9).unwrap();
        for i in expected {
            assert_eq!(t.chunks()[i].revision, 2);
        }
    }
}

#[test]
fn aligned_map_outer_corners_and_invalid_edit() {
    let mut t = Terrain::new(64, 32, 32).unwrap();
    t.clear_dirty();
    t.set_height(64, 32, 1).unwrap();
    assert!(!t.chunks()[0].dirty);
    assert!(t.chunks()[1].dirty);
    let heights = t.heights().to_vec();
    let chunks = t.chunks().to_vec();
    assert!(t.set_height(65, 32, 4).is_err());
    assert_eq!(t.heights(), heights);
    assert_eq!(t.chunks(), chunks);
    let mut tiny = Terrain::new(1, 1, 32).unwrap();
    for (x, y) in [(0, 0), (1, 0), (0, 1), (1, 1)] {
        tiny.set_height(x, y, 2).unwrap();
    }
    assert_eq!(tiny.chunks()[0].revision, 4);
}

#[test]
fn soa_integrity_stable_ids_and_correct_movement() {
    let mut sim = Simulation::new(config(2)).unwrap();
    let ids = sim.vehicles().ids().to_vec();
    let x = sim.vehicles().x().to_vec();
    let y = sim.vehicles().y().to_vec();
    for array in [
        sim.vehicles().x(),
        sim.vehicles().y(),
        sim.vehicles().vx(),
        sim.vehicles().vy(),
    ] {
        assert_eq!(array.len(), ids.len());
    }
    assert_eq!(ids, (0..4096).collect::<Vec<u64>>());
    sim.tick().unwrap();
    assert_eq!(sim.tick_count(), 1);
    for i in 0..ids.len() {
        assert_eq!(
            sim.vehicles().x()[i],
            (x[i] + sim.vehicles().vx()[i] * 20).rem_euclid(65 * 1024)
        );
        assert_eq!(
            sim.vehicles().y()[i],
            (y[i] + sim.vehicles().vy()[i] * 20).rem_euclid(35 * 1024)
        );
    }
    for _ in 0..100 {
        sim.tick().unwrap();
    }
    assert_eq!(sim.vehicles().ids(), ids);
    assert_eq!(sim.tick_count(), 101);
    assert!(
        sim.vehicles()
            .x()
            .iter()
            .all(|&x| (0..65 * 1024).contains(&x))
    );
    assert!(
        sim.vehicles()
            .y()
            .iter()
            .all(|&y| (0..35 * 1024).contains(&y))
    );
}

#[test]
fn same_seed_and_different_thread_counts_match_full_state() {
    let mut reference = Simulation::new(config(1)).unwrap();
    for _ in 0..30 {
        reference.tick().unwrap();
    }
    for threads in [1, 2, 4] {
        let mut sim = Simulation::new(config(threads)).unwrap();
        for _ in 0..30 {
            sim.tick().unwrap();
        }
        assert_eq!(sim.checksum(), reference.checksum());
        assert_eq!(sim.vehicles().x(), reference.vehicles().x());
        assert_eq!(sim.vehicles().y(), reference.vehicles().y());
        assert_eq!(sim.vehicles().vx(), reference.vehicles().vx());
        assert_eq!(sim.vehicles().vy(), reference.vehicles().vy());
        assert_eq!(sim.terrain().heights(), reference.terrain().heights());
    }
    let mut other = config(1);
    other.seed = 43;
    assert_ne!(
        Simulation::new(other).unwrap().vehicles().x(),
        Simulation::new(config(1)).unwrap().vehicles().x()
    );
}

#[test]
fn empty_simulation_and_on_demand_diagnostics() {
    let mut c = config(1);
    c.vehicles = 0;
    let mut sim = Simulation::new(c).unwrap();
    assert!(sim.vehicles().is_empty());
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
    sim.set_height(32, 32, 5).unwrap();
    assert_ne!(before, sim.checksum());
}

#[test]
fn supports_one_hundred_thousand_vehicles() {
    let mut c = config(4);
    c.vehicles = 100_000;
    let mut sim = Simulation::new(c).unwrap();
    sim.tick().unwrap();
    assert_eq!(sim.vehicles().len(), 100_000);
    assert_eq!(sim.vehicles().ids()[99_999], 99_999);
}
