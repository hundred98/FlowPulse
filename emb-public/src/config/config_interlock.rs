//! Configuration Interlock Validation
//!
//! Validates cross-section interlock rules for bed mesh configuration.
//! These rules ensure that probe parameters and interpolation algorithm
//! settings are compatible.

use super::config_adapter::BedMeshHardwareConfig;

/// Validate and apply interlock rules for bed mesh configuration.
///
/// Interlock rules (from bed mesh design document):
/// - probe_count_x/y ≥ 6 → algorithm forced to "bicubic"
/// - probe_count_x/y = 6 → mesh_pps_x/y forced to 1
/// - probe_count_x/y ≥ 7 → mesh_pps_x/y forced to 0
///
/// If interlock rules are violated, the configuration is automatically
/// corrected and a warning is logged.
///
/// # Arguments
/// * `bed_mesh` - Bed mesh configuration to validate and correct
///
/// # Returns
/// * `Ok(())` if validation passed (with possible corrections)
/// * `Err(String)` if validation failed with unrecoverable error
pub fn validate_bed_mesh_interlock(bed_mesh: &mut BedMeshHardwareConfig) -> Result<(), String> {
    let probe = &bed_mesh.probe;
    let algorithm = &mut bed_mesh.algorithm;

    // Rule 1: probe_count ≥ 6 → algorithm forced to bicubic
    if probe.probe_count_x >= 6 || probe.probe_count_y >= 6 {
        if algorithm.algorithm != "bicubic" {
            tracing::warn!(
                "Bed mesh interlock: probe_count_x={}, probe_count_y={} ≥ 6, algorithm '{}' forced to 'bicubic'",
                probe.probe_count_x,
                probe.probe_count_y,
                algorithm.algorithm
            );
            algorithm.algorithm = "bicubic".to_string();
        }
    }

    // Rule 2: probe_count_x = 6 → mesh_pps_x forced to 1
    if probe.probe_count_x == 6 {
        if algorithm.mesh_pps_x != 1 {
            tracing::warn!(
                "Bed mesh interlock: probe_count_x=6, mesh_pps_x={} forced to 1",
                algorithm.mesh_pps_x
            );
            algorithm.mesh_pps_x = 1;
        }
    }

    // Rule 3: probe_count_x ≥ 7 → mesh_pps_x forced to 0
    if probe.probe_count_x >= 7 {
        if algorithm.mesh_pps_x != 0 {
            tracing::warn!(
                "Bed mesh interlock: probe_count_x={} ≥ 7, mesh_pps_x={} forced to 0",
                probe.probe_count_x,
                algorithm.mesh_pps_x
            );
            algorithm.mesh_pps_x = 0;
        }
    }

    // Rule 4: probe_count_y = 6 → mesh_pps_y forced to 1
    if probe.probe_count_y == 6 {
        if algorithm.mesh_pps_y != 1 {
            tracing::warn!(
                "Bed mesh interlock: probe_count_y=6, mesh_pps_y={} forced to 1",
                algorithm.mesh_pps_y
            );
            algorithm.mesh_pps_y = 1;
        }
    }

    // Rule 5: probe_count_y ≥ 7 → mesh_pps_y forced to 0
    if probe.probe_count_y >= 7 {
        if algorithm.mesh_pps_y != 0 {
            tracing::warn!(
                "Bed mesh interlock: probe_count_y={} ≥ 7, mesh_pps_y={} forced to 0",
                probe.probe_count_y,
                algorithm.mesh_pps_y
            );
            algorithm.mesh_pps_y = 0;
        }
    }

    // Basic validation: probe_count must be in 1-8 range
    if probe.probe_count_x < 1 || probe.probe_count_x > 8 {
        return Err(format!(
            "probe_count_x must be in 1-8 range, got {}",
            probe.probe_count_x
        ));
    }
    if probe.probe_count_y < 1 || probe.probe_count_y > 8 {
        return Err(format!(
            "probe_count_y must be in 1-8 range, got {}",
            probe.probe_count_y
        ));
    }

    // Basic validation: mesh_min < mesh_max
    if probe.mesh_min_x >= probe.mesh_max_x {
        return Err(format!(
            "mesh_min_x ({}) must be less than mesh_max_x ({})",
            probe.mesh_min_x, probe.mesh_max_x
        ));
    }
    if probe.mesh_min_y >= probe.mesh_max_y {
        return Err(format!(
            "mesh_min_y ({}) must be less than mesh_max_y ({})",
            probe.mesh_min_y, probe.mesh_max_y
        ));
    }

    // Basic validation: fade_start != fade_end (equal视为配置错误)
    if algorithm.fade_start == algorithm.fade_end {
        tracing::warn!(
            "Bed mesh config: fade_start ({}) equals fade_end ({}), this is a config error",
            algorithm.fade_start,
            algorithm.fade_end
        );
        // 不阻塞启动，但记录警告
    }

    // Basic validation: algorithm must be "lagrange", "bicubic", or "bilinear"
    if algorithm.algorithm != "lagrange" && algorithm.algorithm != "bicubic" && algorithm.algorithm != "bilinear" {
        return Err(format!(
            "algorithm must be 'lagrange', 'bicubic', or 'bilinear', got '{}'",
            algorithm.algorithm
        ));
    }

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_interlock_probe_count_5() {
        // 5x5 mesh, no interlock
        let mut bed_mesh = BedMeshHardwareConfig::default();
        bed_mesh.probe.probe_count_x = 5;
        bed_mesh.probe.probe_count_y = 5;
        bed_mesh.algorithm.algorithm = "lagrange".to_string();
        bed_mesh.algorithm.mesh_pps_x = 2;
        bed_mesh.algorithm.mesh_pps_y = 2;

        validate_bed_mesh_interlock(&mut bed_mesh).unwrap();

        // No changes expected
        assert_eq!(bed_mesh.algorithm.algorithm, "lagrange");
        assert_eq!(bed_mesh.algorithm.mesh_pps_x, 2);
        assert_eq!(bed_mesh.algorithm.mesh_pps_y, 2);
    }

    #[test]
    fn test_interlock_probe_count_6() {
        // 6x6 mesh, algorithm forced to bicubic, mesh_pps forced to 1
        let mut bed_mesh = BedMeshHardwareConfig::default();
        bed_mesh.probe.probe_count_x = 6;
        bed_mesh.probe.probe_count_y = 6;
        bed_mesh.algorithm.algorithm = "lagrange".to_string();
        bed_mesh.algorithm.mesh_pps_x = 2;
        bed_mesh.algorithm.mesh_pps_y = 2;

        validate_bed_mesh_interlock(&mut bed_mesh).unwrap();

        // Changes expected
        assert_eq!(bed_mesh.algorithm.algorithm, "bicubic");
        assert_eq!(bed_mesh.algorithm.mesh_pps_x, 1);
        assert_eq!(bed_mesh.algorithm.mesh_pps_y, 1);
    }

    #[test]
    fn test_interlock_probe_count_7() {
        // 7x7 mesh, algorithm forced to bicubic, mesh_pps forced to 0
        let mut bed_mesh = BedMeshHardwareConfig::default();
        bed_mesh.probe.probe_count_x = 7;
        bed_mesh.probe.probe_count_y = 7;
        bed_mesh.algorithm.algorithm = "lagrange".to_string();
        bed_mesh.algorithm.mesh_pps_x = 2;
        bed_mesh.algorithm.mesh_pps_y = 2;

        validate_bed_mesh_interlock(&mut bed_mesh).unwrap();

        // Changes expected
        assert_eq!(bed_mesh.algorithm.algorithm, "bicubic");
        assert_eq!(bed_mesh.algorithm.mesh_pps_x, 0);
        assert_eq!(bed_mesh.algorithm.mesh_pps_y, 0);
    }

    #[test]
    fn test_interlock_probe_count_invalid() {
        // probe_count = 0, should fail
        let mut bed_mesh = BedMeshHardwareConfig::default();
        bed_mesh.probe.probe_count_x = 0;

        let result = validate_bed_mesh_interlock(&mut bed_mesh);
        assert!(result.is_err());
    }

    #[test]
    fn test_interlock_mesh_range_invalid() {
        // mesh_min >= mesh_max, should fail
        let mut bed_mesh = BedMeshHardwareConfig::default();
        bed_mesh.probe.mesh_min_x = 100.0;
        bed_mesh.probe.mesh_max_x = 50.0;

        let result = validate_bed_mesh_interlock(&mut bed_mesh);
        assert!(result.is_err());
    }
}

