use sysinfo::{Pid, System};

pub fn process_name(process_id: u32) -> Option<String> {
    let mut system = System::new_all();
    system.refresh_processes_specifics(
        sysinfo::ProcessesToUpdate::All,
        true,
        sysinfo::ProcessRefreshKind::nothing().with_exe(sysinfo::UpdateKind::Always),
    );
    system
        .process(Pid::from_u32(process_id))
        .map(|process| process.name().to_string_lossy().to_string())
}

pub fn process_label(process_id: u32) -> String {
    process_name(process_id)
        .map(|name| format!("{name} ({process_id})"))
        .unwrap_or_else(|| format!("Processus {process_id}"))
}
