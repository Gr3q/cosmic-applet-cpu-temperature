use std::{cmp::Ordering, i32::MAX};

use once_cell::sync::Lazy;
use regex::Regex;
use sysinfo::{Component, Components};

// In order of priority
const OVERALL_CPU_TEMP_LABELS: &'static [&'static str] = &[
    // AMD CPUs
    "Tctl",
    // Intel CPUs
    "Package id 0",
    // CPU Temp from some motherboards
    "CPU Temperature",
];

static INTEL_CPU_REGEX: Lazy<Regex> = Lazy::new(|| Regex::new(r"^CPU [\d]{1}$").unwrap());
static AMD_CPU_REGEX: Lazy<Regex> = Lazy::new(|| Regex::new(r"^Tctl[\d]{1}$").unwrap());

// Returns -1 if not found, priority otherwise.
// Lower number means higher priority
fn get_overall_temperature_component_index(component: &Component) -> i32 {
    let mut index: i32 = -1;
    for item in OVERALL_CPU_TEMP_LABELS {
        if *item == component.label() {
            return index;
        }

        index = index + 1;
    }

    return index;
}

fn get_overall_cpu_temp(components: &Components) -> Option<f32> {
    let mut target: Option<&Component> = None;
    let mut current_priority = MAX;
    for comp in components.iter() {
        let priority = get_overall_temperature_component_index(comp);
        // Not a component we need
        if priority == -1 {
            continue;
        }

        // Doesn't provide temperature
        if comp.temperature().is_none() {
            continue;
        }

        // Lower priority over what we found already
        if priority > current_priority {
            continue;
        }

        target = Some(comp);
        current_priority = priority;
    }

    if let Some(target_exists) = target {
        return target_exists.temperature();
    }

    return None;
}

fn get_cpu_core_temps(components: &Components) -> Vec<f32> {
    let mut temps: Vec<f32> = vec![];
    for comp in components.iter() {
        let mut temp: Option<f32> = None;
        if INTEL_CPU_REGEX.is_match(comp.label()) {
            temp = comp.temperature();
        } else if AMD_CPU_REGEX.is_match(comp.label()) {
            temp = comp.temperature();
        }

        if let Some(valid_temp) = temp {
            temps.push(valid_temp);
        }
    }

    return temps;
}

pub(crate) fn get_temp() -> Option<f32> {
    let components = Components::new_with_refreshed_list();
    let overall_temp = get_overall_cpu_temp(&components);
    if overall_temp.is_some() {
        return overall_temp;
    }

    let cpu_temps = get_cpu_core_temps(&components);
    if cpu_temps.len() == 0 {
        return None;
    }

    return cpu_temps
        .iter()
        .max_by(|a, b| {
            if a > b {
                Ordering::Greater
            } else {
                Ordering::Less
            }
        })
        .copied();
}
