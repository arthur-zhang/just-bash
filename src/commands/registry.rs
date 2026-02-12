// src/commands/registry.rs
use std::collections::HashMap;
use super::types::Command;

pub struct CommandRegistry {
    commands: HashMap<String, Box<dyn Command>>,
}

impl CommandRegistry {
    pub fn new() -> Self {
        Self {
            commands: HashMap::new(),
        }
    }

    pub fn register(&mut self, cmd: Box<dyn Command>) {
        self.commands.insert(cmd.name().to_string(), cmd);
    }

    pub fn get(&self, name: &str) -> Option<&dyn Command> {
        self.commands.get(name).map(|c| c.as_ref())
    }

    pub fn names(&self) -> Vec<&str> {
        self.commands.keys().map(|s| s.as_str()).collect()
    }

    pub fn contains(&self, name: &str) -> bool {
        self.commands.contains_key(name)
    }
}

impl Default for CommandRegistry {
    fn default() -> Self {
        Self::new()
    }
}

use super::basename::BasenameCommand;
use super::dirname::DirnameCommand;
use super::cat::CatCommand;
use super::head::HeadCommand;
use super::tail::TailCommand;
use super::wc::WcCommand;
use super::mkdir::MkdirCommand;
use super::touch::TouchCommand;
use super::rm::RmCommand;
use super::cp::CpCommand;
use super::mv::MvCommand;
use super::ls::LsCommand;
use super::grep::GrepCommand;
use super::test_cmd::{TestCommand, BracketCommand};

// Batch B imports
use super::uniq::UniqCommand;
use super::cut::CutCommand;
use super::nl::NlCommand;
use super::tr::TrCommand;
use super::paste::PasteCommand;
use super::join::JoinCommand;
use super::sort::SortCommand;
use super::sed::SedCommand;
use super::awk::AwkCommand;

// Batch C imports
use super::jq::JqCommand;
use super::yq::YqCommand;

// Batch D imports
use super::base64_cmd::Base64Command;
use super::diff_cmd::DiffCommand;
use super::gzip::{GzipCommand, GunzipCommand, ZcatCommand};
use super::find::FindCommand;
use super::tar::TarCommand;
use super::xargs::XargsCommand;
use super::curl::CurlCommand;

// Batch E imports
use super::echo::EchoCommand;
use super::env::{EnvCommand, PrintenvCommand};
use super::printf::PrintfCommand;
use super::pwd::PwdCommand;
use super::ln::LnCommand;
use super::chmod::ChmodCommand;
use super::date::DateCommand;

// Batch F imports
use super::md5sum::{Md5sumCommand, Sha1sumCommand, Sha256sumCommand};
use super::stat_cmd::StatCommand;
use super::seq::SeqCommand;
use super::tee::TeeCommand;
use super::sleep_cmd::SleepCommand;
use super::split_cmd::SplitCommand;

// Batch G imports
use super::true_cmd::{TrueCommand, FalseCommand};
use super::clear_cmd::ClearCommand;
use super::whoami_cmd::WhoamiCommand;
use super::hostname_cmd::HostnameCommand;
use super::rmdir_cmd::RmdirCommand;
use super::tac_cmd::TacCommand;
use super::rev_cmd::RevCommand;

// Batch H imports
use super::readlink_cmd::ReadlinkCommand;
use super::which_cmd::WhichCommand;
use super::time_cmd::TimeCommand;
use super::expand_cmd::ExpandCommand;
use super::fold_cmd::FoldCommand;
use super::strings_cmd::StringsCommand;

// Batch I imports
use super::column_cmd::ColumnCommand;
use super::comm_cmd::CommCommand;

// Batch J imports
use super::timeout_cmd::TimeoutCommand;
use super::tree_cmd::TreeCommand;
use super::expr_cmd::ExprCommand;

// Batch K imports
use super::od_cmd::OdCommand;
use super::du_cmd::DuCommand;
use super::file_cmd::FileCommand;

// Batch L imports
use super::alias_cmd::AliasCommand;
use super::unalias_cmd::UnaliasCommand;
use super::history_cmd::HistoryCommand;
use super::bash_cmd::{BashCommand, ShCommand};
use super::help_cmd::HelpCommand;

/// 注册批次 A 的所有命令
pub fn register_batch_a(registry: &mut CommandRegistry) {
    registry.register(Box::new(BasenameCommand));
    registry.register(Box::new(DirnameCommand));
    registry.register(Box::new(CatCommand));
    registry.register(Box::new(HeadCommand));
    registry.register(Box::new(TailCommand));
    registry.register(Box::new(WcCommand));
    registry.register(Box::new(MkdirCommand));
    registry.register(Box::new(TouchCommand));
    registry.register(Box::new(RmCommand));
    registry.register(Box::new(CpCommand));
    registry.register(Box::new(MvCommand));
    registry.register(Box::new(LsCommand));
    registry.register(Box::new(GrepCommand));
    registry.register(Box::new(TestCommand));
    registry.register(Box::new(BracketCommand));
}

/// 创建包含批次 A 命令的注册表
pub fn create_batch_a_registry() -> CommandRegistry {
    let mut registry = CommandRegistry::new();
    register_batch_a(&mut registry);
    registry
}

/// 注册批次 B 的所有命令
pub fn register_batch_b(registry: &mut CommandRegistry) {
    registry.register(Box::new(UniqCommand));
    registry.register(Box::new(CutCommand));
    registry.register(Box::new(NlCommand));
    registry.register(Box::new(TrCommand));
    registry.register(Box::new(PasteCommand));
    registry.register(Box::new(JoinCommand));
    registry.register(Box::new(SortCommand));
    registry.register(Box::new(SedCommand));
    registry.register(Box::new(AwkCommand));
}

/// 创建包含批次 A 和 B 命令的注册表
pub fn create_batch_ab_registry() -> CommandRegistry {
    let mut registry = CommandRegistry::new();
    register_batch_a(&mut registry);
    register_batch_b(&mut registry);
    registry
}

/// 注册批次 C 的所有命令
pub fn register_batch_c(registry: &mut CommandRegistry) {
    registry.register(Box::new(JqCommand));
    registry.register(Box::new(YqCommand));
}

/// 创建包含批次 A、B 和 C 命令的注册表
pub fn create_batch_abc_registry() -> CommandRegistry {
    let mut registry = CommandRegistry::new();
    register_batch_a(&mut registry);
    register_batch_b(&mut registry);
    register_batch_c(&mut registry);
    registry
}

/// 注册批次 D 的所有命令
pub fn register_batch_d(registry: &mut CommandRegistry) {
    registry.register(Box::new(Base64Command));
    registry.register(Box::new(DiffCommand));
    registry.register(Box::new(GzipCommand));
    registry.register(Box::new(GunzipCommand));
    registry.register(Box::new(ZcatCommand));
    registry.register(Box::new(FindCommand));
    registry.register(Box::new(TarCommand));
    registry.register(Box::new(XargsCommand));
    registry.register(Box::new(CurlCommand));
}

/// 创建包含批次 A、B、C 和 D 命令的注册表
pub fn create_batch_abcd_registry() -> CommandRegistry {
    let mut registry = CommandRegistry::new();
    register_batch_a(&mut registry);
    register_batch_b(&mut registry);
    register_batch_c(&mut registry);
    register_batch_d(&mut registry);
    registry
}

/// 注册批次 E 的所有命令 (echo, env, printf, pwd, ln, chmod, date)
pub fn register_batch_e(registry: &mut CommandRegistry) {
    registry.register(Box::new(EchoCommand));
    registry.register(Box::new(EnvCommand));
    registry.register(Box::new(PrintenvCommand));
    registry.register(Box::new(PrintfCommand));
    registry.register(Box::new(PwdCommand));
    registry.register(Box::new(LnCommand));
    registry.register(Box::new(ChmodCommand));
    registry.register(Box::new(DateCommand));
}

/// 创建包含批次 A、B、C、D 和 E 命令的注册表
pub fn create_batch_abcde_registry() -> CommandRegistry {
    let mut registry = CommandRegistry::new();
    register_batch_a(&mut registry);
    register_batch_b(&mut registry);
    register_batch_c(&mut registry);
    register_batch_d(&mut registry);
    register_batch_e(&mut registry);
    registry
}

/// 注册批次 F 的所有命令 (md5sum, sha1sum, sha256sum, stat, seq, tee, sleep, split)
pub fn register_batch_f(registry: &mut CommandRegistry) {
    registry.register(Box::new(Md5sumCommand));
    registry.register(Box::new(Sha1sumCommand));
    registry.register(Box::new(Sha256sumCommand));
    registry.register(Box::new(StatCommand));
    registry.register(Box::new(SeqCommand));
    registry.register(Box::new(TeeCommand));
    registry.register(Box::new(SleepCommand));
    registry.register(Box::new(SplitCommand));
}

/// 创建包含批次 A-F 命令的注册表
pub fn create_batch_abcdef_registry() -> CommandRegistry {
    let mut registry = CommandRegistry::new();
    register_batch_a(&mut registry);
    register_batch_b(&mut registry);
    register_batch_c(&mut registry);
    register_batch_d(&mut registry);
    register_batch_e(&mut registry);
    register_batch_f(&mut registry);
    registry
}

/// 注册批次 G 的所有命令 (true, false, clear, whoami, hostname, rmdir, tac, rev)
pub fn register_batch_g(registry: &mut CommandRegistry) {
    registry.register(Box::new(TrueCommand));
    registry.register(Box::new(FalseCommand));
    registry.register(Box::new(ClearCommand));
    registry.register(Box::new(WhoamiCommand));
    registry.register(Box::new(HostnameCommand));
    registry.register(Box::new(RmdirCommand));
    registry.register(Box::new(TacCommand));
    registry.register(Box::new(RevCommand));
}

/// 创建包含批次 A-G 命令的注册表
pub fn create_batch_abcdefg_registry() -> CommandRegistry {
    let mut registry = CommandRegistry::new();
    register_batch_a(&mut registry);
    register_batch_b(&mut registry);
    register_batch_c(&mut registry);
    register_batch_d(&mut registry);
    register_batch_e(&mut registry);
    register_batch_f(&mut registry);
    register_batch_g(&mut registry);
    registry
}

/// 注册批次 H 的所有命令 (readlink, which, time, expand, fold, strings)
pub fn register_batch_h(registry: &mut CommandRegistry) {
    registry.register(Box::new(ReadlinkCommand));
    registry.register(Box::new(WhichCommand));
    registry.register(Box::new(TimeCommand));
    registry.register(Box::new(ExpandCommand));
    registry.register(Box::new(FoldCommand));
    registry.register(Box::new(StringsCommand));
}

/// 创建包含批次 A-H 命令的注册表
pub fn create_batch_abcdefgh_registry() -> CommandRegistry {
    let mut registry = CommandRegistry::new();
    register_batch_a(&mut registry);
    register_batch_b(&mut registry);
    register_batch_c(&mut registry);
    register_batch_d(&mut registry);
    register_batch_e(&mut registry);
    register_batch_f(&mut registry);
    register_batch_g(&mut registry);
    register_batch_h(&mut registry);
    registry
}

/// 注册批次 I 的所有命令 (column, comm)
pub fn register_batch_i(registry: &mut CommandRegistry) {
    registry.register(Box::new(ColumnCommand));
    registry.register(Box::new(CommCommand));
}

/// 创建包含批次 A-I 命令的注册表
pub fn create_batch_abcdefghi_registry() -> CommandRegistry {
    let mut registry = CommandRegistry::new();
    register_batch_a(&mut registry);
    register_batch_b(&mut registry);
    register_batch_c(&mut registry);
    register_batch_d(&mut registry);
    register_batch_e(&mut registry);
    register_batch_f(&mut registry);
    register_batch_g(&mut registry);
    register_batch_h(&mut registry);
    register_batch_i(&mut registry);
    registry
}

/// 注册批次 J 的所有命令 (timeout, tree, expr)
pub fn register_batch_j(registry: &mut CommandRegistry) {
    registry.register(Box::new(TimeoutCommand));
    registry.register(Box::new(TreeCommand));
    registry.register(Box::new(ExprCommand));
}

/// 创建包含批次 A-J 命令的注册表
pub fn create_batch_abcdefghij_registry() -> CommandRegistry {
    let mut registry = CommandRegistry::new();
    register_batch_a(&mut registry);
    register_batch_b(&mut registry);
    register_batch_c(&mut registry);
    register_batch_d(&mut registry);
    register_batch_e(&mut registry);
    register_batch_f(&mut registry);
    register_batch_g(&mut registry);
    register_batch_h(&mut registry);
    register_batch_i(&mut registry);
    register_batch_j(&mut registry);
    registry
}

/// 注册批次 K 的所有命令 (od, du, file)
pub fn register_batch_k(registry: &mut CommandRegistry) {
    registry.register(Box::new(OdCommand));
    registry.register(Box::new(DuCommand));
    registry.register(Box::new(FileCommand));
}

/// 创建包含批次 A-K 命令的注册表
pub fn create_batch_abcdefghijk_registry() -> CommandRegistry {
    let mut registry = CommandRegistry::new();
    register_batch_a(&mut registry);
    register_batch_b(&mut registry);
    register_batch_c(&mut registry);
    register_batch_d(&mut registry);
    register_batch_e(&mut registry);
    register_batch_f(&mut registry);
    register_batch_g(&mut registry);
    register_batch_h(&mut registry);
    register_batch_i(&mut registry);
    register_batch_j(&mut registry);
    register_batch_k(&mut registry);
    registry
}

/// 注册批次 L 的所有命令 (alias, unalias, history, bash, sh, help)
pub fn register_batch_l(registry: &mut CommandRegistry) {
    registry.register(Box::new(AliasCommand));
    registry.register(Box::new(UnaliasCommand));
    registry.register(Box::new(HistoryCommand));
    registry.register(Box::new(BashCommand));
    registry.register(Box::new(ShCommand));
    registry.register(Box::new(HelpCommand));
}

/// 创建包含批次 A-L 命令的注册表
pub fn create_batch_abcdefghijkl_registry() -> CommandRegistry {
    let mut registry = CommandRegistry::new();
    register_batch_a(&mut registry);
    register_batch_b(&mut registry);
    register_batch_c(&mut registry);
    register_batch_d(&mut registry);
    register_batch_e(&mut registry);
    register_batch_f(&mut registry);
    register_batch_g(&mut registry);
    register_batch_h(&mut registry);
    register_batch_i(&mut registry);
    register_batch_j(&mut registry);
    register_batch_k(&mut registry);
    register_batch_l(&mut registry);
    registry
}

// Batch M imports
use super::rg_cmd::RgCommand;

/// 注册批次 M 的所有命令 (rg)
pub fn register_batch_m(registry: &mut CommandRegistry) {
    registry.register(Box::new(RgCommand));
}

/// 创建包含批次 A-M 命令的注册表
pub fn create_batch_abcdefghijklm_registry() -> CommandRegistry {
    let mut registry = CommandRegistry::new();
    register_batch_a(&mut registry);
    register_batch_b(&mut registry);
    register_batch_c(&mut registry);
    register_batch_d(&mut registry);
    register_batch_e(&mut registry);
    register_batch_f(&mut registry);
    register_batch_g(&mut registry);
    register_batch_h(&mut registry);
    register_batch_i(&mut registry);
    register_batch_j(&mut registry);
    register_batch_k(&mut registry);
    register_batch_l(&mut registry);
    register_batch_m(&mut registry);
    registry
}

// Batch N imports
use super::html_to_markdown_cmd::HtmlToMarkdownCommand;

/// 注册批次 N 的所有命令 (html-to-markdown)
pub fn register_batch_n(registry: &mut CommandRegistry) {
    registry.register(Box::new(HtmlToMarkdownCommand));
}

/// 创建包含批次 A-N 命令的注册表
pub fn create_batch_abcdefghijklmn_registry() -> CommandRegistry {
    let mut registry = CommandRegistry::new();
    register_batch_a(&mut registry);
    register_batch_b(&mut registry);
    register_batch_c(&mut registry);
    register_batch_d(&mut registry);
    register_batch_e(&mut registry);
    register_batch_f(&mut registry);
    register_batch_g(&mut registry);
    register_batch_h(&mut registry);
    register_batch_i(&mut registry);
    register_batch_j(&mut registry);
    register_batch_k(&mut registry);
    register_batch_l(&mut registry);
    register_batch_m(&mut registry);
    register_batch_n(&mut registry);
    registry
}
