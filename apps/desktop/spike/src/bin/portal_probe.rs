//! Phase 0 清单第 3/4 条：XDG portal 能力探针（仅 Linux，需 linux-portals feature）。
//! 会真实弹出系统授权框——弹窗行为本身就是要记录的数据。
//! 注意：ashpd API 随版本变动，编译错误按所装版本文档修正即可，不算探针失败。
use anyhow::{Context, Result};

#[tokio::main(flavor = "current_thread")]
async fn main() {
    println!("== 会话环境 ==");
    for key in ["XDG_SESSION_TYPE", "XDG_CURRENT_DESKTOP", "DESKTOP_SESSION"] {
        println!("{key}={}", std::env::var(key).unwrap_or_else(|_| "<unset>".into()));
    }
    probe_screencast().await;
    probe_global_shortcuts().await;
    println!("== 完成，把以上输出贴入 phase0-checklist.md 记录表 ==");
}

async fn probe_screencast() {
    println!("== ScreenCast portal（清单第 3 条）==");
    match screencast_inner().await {
        Ok(note) => println!("SCREENCAST: AVAILABLE · {note}"),
        Err(err) => println!("SCREENCAST: ERROR · {err:#}"),
    }
}

async fn screencast_inner() -> Result<String> {
    use ashpd::desktop::screencast::{CursorMode, Screencast, SourceType};
    let proxy = Screencast::new().await.context("创建 Screencast 代理失败")?;
    let session = proxy.create_session().await.context("CreateSession 失败")?;
    // persist_mode：尽量请求持久授权（restore token 由桌面门户决定是否支持）
    proxy
        .select_sources(
            &session,
            CursorMode::Hidden,
            SourceType::Monitor.into(),
            false,
            None,
            ashpd::desktop::PersistMode::ExplicitlyRevoked,
        )
        .await
        .context("SelectSources 失败")?;
    let request = proxy.start(&session, None).await.context("Start 失败")?;
    let response = request.response().context("等待用户授权失败（被拒绝？")?;
    let streams = response.streams();
    let first = streams.first().context("未返回任何流")?;
    Ok(format!(
        "{} 个流，pipewire_node={}，大小={:?}",
        streams.len(),
        first.pipe_wire_node_id(),
        first.size()
    ))
}

async fn probe_global_shortcuts() {
    println!("== GlobalShortcuts portal（清单第 4 条）==");
    match global_shortcuts_inner().await {
        Ok(note) => println!("GLOBAL_SHORTCUTS: AVAILABLE · {note}"),
        Err(err) => println!("GLOBAL_SHORTCUTS: UNSUPPORTED/ERROR · {err:#}"),
    }
}

async fn global_shortcuts_inner() -> Result<String> {
    use ashpd::desktop::global_shortcuts::GlobalShortcuts;
    let proxy = GlobalShortcuts::new().await.context("创建 GlobalShortcuts 代理失败")?;
    let session = proxy.create_session().await.context("CreateSession 失败")?;
    // 只探测创建与会话能力，不实际绑定按键；绑定在 Phase 3 完成。
    drop(session);
    Ok("代理与会话创建成功".to_string())
}
