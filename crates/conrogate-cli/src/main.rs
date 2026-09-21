//! Conrogate 唯一二进制入口。
//!
//! 通过子命令分发到不同运行模式：
//!   conrogate serve     # 合并模式（数据面 + 控制面同进程）
//!   conrogate gate      # 分离模式数据面
//!   conrogate control   # 分离模式控制面
//!   conrogate migrate   # 数据库迁移（+ --seed 演示数据）

mod bootstrap;
mod cmd_control;
mod cmd_gate;
mod cmd_migrate;
mod cmd_serve;
mod http_config_loader;

use clap::{Parser, Subcommand};

#[derive(Parser)]
#[command(name = "conrogate")]
#[command(about = "Conrogate 轻量级微服务网关")]
struct Cli {
    /// 指定 .env 文件路径
    #[arg(long, global = true)]
    env_file: Option<String>,

    #[command(subcommand)]
    command: Commands,
}

#[derive(Subcommand)]
enum Commands {
    /// 合并模式（数据面 8080 + 控制面 9000 同进程）
    Serve,
    /// 分离模式：仅数据面（8080）
    Gate,
    /// 分离模式：仅控制面（9000）
    Control,
    /// 数据库迁移（+ --seed 写入演示数据）
    Migrate {
        /// 迁移后写入演示数据
        #[arg(long)]
        seed: bool,
        /// 演示上游名称（仅在 --seed 时生效）
        #[arg(long, default_value = "echo-upstream")]
        seed_name: String,
        /// 演示上游地址（仅在 --seed 时生效）
        #[arg(long, default_value = "127.0.0.1:9090")]
        seed_address: String,
    },
}

fn main() -> anyhow::Result<()> {
    let cli = Cli::parse();

    // 加载 .env（可选）
    if let Some(path) = cli.env_file {
        let _ = dotenvy::from_path(&path);
    } else {
        let _ = dotenvy::dotenv();
    }

    // 加载配置
    let config = conrogate_core::contract::config::Config::from_env()
        .map_err(|e| anyhow::anyhow!("config load failed: {e}"))?;
    config
        .validate()
        .map_err(|e| anyhow::anyhow!("config validation failed: {e}"))?;

    // 初始化日志
    conrogate_core::logging::init(&config.log);

    match cli.command {
        Commands::Serve => cmd_serve::run(config),
        Commands::Gate => cmd_gate::run(config),
        Commands::Control => cmd_control::run(config),
        Commands::Migrate {
            seed,
            seed_name,
            seed_address,
        } => cmd_migrate::run(config, seed, seed_name, seed_address),
    }
}
