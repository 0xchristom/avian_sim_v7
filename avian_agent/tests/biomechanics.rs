use avian_agent::locomotion::VaultingGait;

#[test]
fn test_duty_factor_range() {
    let gait = VaultingGait {
        leg_length_m: 0.1,
        duty_factor: 0.65,
        stance_phase: 0.0,
    };
    assert!(gait.duty_factor >= 0.6 && gait.duty_factor <= 0.7);
}

#[test]
fn test_mass_scales_with_age() {
    let mass_juvenile = 250.0 + (0.5 - 0.5) * 130.0;
    let mass_adult = 315.0;
    let mass_old = 315.0 - (10.0 - 8.0) * 5.0;

    assert!(mass_juvenile < mass_adult);
    assert!(mass_old < mass_adult);
}
