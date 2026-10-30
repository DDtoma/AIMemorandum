//! Phase 0 清单第 2/5 条：内存基线 + 全屏框选层形态。
//!   memory_probe            空闲模式：不显示窗口，仅事件循环（模拟托盘常驻）
//!   memory_probe --window   显示模拟记录窗口（50 条假数据）
//!   PROBE_FULLSCREEN=1      叠加全屏无边框（配合 --window，验证 GNOME/KDE 框选层）
//! 采样脚本：scripts/measure-memory.sh（Linux）/ scripts/measure-memory.ps1（Windows）
slint::include_modules!();

fn main() -> anyhow::Result<()> {
    let windowed = std::env::args().any(|a| a == "--window");
    if !windowed {
        println!("空闲模式：无窗口事件循环。用 measure-memory 脚本对本进程采样后 Ctrl-C 结束。");
        slint::run_event_loop()?;
        return Ok(());
    }
    let ui = MemoryProbe::new()?;
    let rows: Vec<slint::SharedString> = (1..=50)
        .map(|i| slint::SharedString::from(format!("待办 {i}：确认第 {i} 项预算数字")))
        .collect();
    ui.set_rows(slint::ModelRc::new(slint::VecModel::from(rows)));
    if std::env::var_os("PROBE_FULLSCREEN").is_some() {
        ui.window().set_fullscreen(true);
    }
    ui.run()?;
    Ok(())
}
