//! Phase 0 清单第 1 条：中文 IME 组字期间 Enter 不误提交。
//! 判定方式：探针把 LineEdit 的 accepted 事件记入日志；
//! 组字中按 Enter 若产生日志即为"误提交"，本项不通过。
slint::include_modules!();

fn main() -> anyhow::Result<()> {
    let ui = ImeProbe::new()?;
    let weak = ui.as_weak();
    ui.on_submitted(move |text| {
        if let Some(ui) = weak.upgrade() {
            let stamp = std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .map(|d| d.as_millis())
                .unwrap_or(0);
            let mut log = ui.get_log().to_string();
            log.push_str(&format!("[submit] {:?} @{}\n", text, stamp));
            ui.set_log(log.into());
        }
    });
    ui.run()?;
    Ok(())
}
