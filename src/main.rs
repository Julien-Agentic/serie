mod app;
mod check;
mod color;
mod config;
mod event;
mod external;
mod git;
mod graph;
mod keybind;
mod protocol;
mod search;
mod view;
mod widget;

#[cfg(test)]
#[path = "tests/mailmap.rs"]
mod mailmap_tests;

#[cfg(test)]
mod test_support;

use std::{path::Path, rc::Rc};

use clap::{Parser, ValueEnum};
use serde::Deserialize;

use crate::{
    app::{App, Ret},
    graph::GraphImageManager,
};

/// Serie - A rich git commit graph in your terminal, like magic 📚
#[derive(Parser)]
#[command(version)]
struct Args {
    /// Maximum number of commits to render
    #[arg(short = 'n', long, value_name = "NUMBER")]
    max_count: Option<usize>,

    /// Image protocol to render graph [default: auto]
    #[arg(short, long, value_name = "TYPE")]
    protocol: Option<ImageProtocolType>,

    /// Commit ordering algorithm [default: chrono]
    #[arg(short, long, value_name = "TYPE")]
    order: Option<CommitOrderType>,

    /// Commit graph image cell width [default: auto]
    #[arg(short, long, value_name = "TYPE")]
    graph_width: Option<GraphWidthType>,

    /// Commit graph image edge style [default: rounded]
    #[arg(short = 's', long, value_name = "TYPE")]
    graph_style: Option<GraphStyle>,

    /// Enable antialiasing for commit graph images
    #[arg(long, conflicts_with = "no_graph_antialias")]
    graph_antialias: bool,

    /// Disable antialiasing for commit graph images
    #[arg(long, conflicts_with = "graph_antialias")]
    no_graph_antialias: bool,

    /// Initial selection of commit [default: latest]
    #[arg(short, long, value_name = "TYPE")]
    initial_selection: Option<InitialSelection>,

    /// Primary branch to keep on the leftmost column
    #[arg(short = 'b', long, value_name = "BRANCH")]
    primary_branch: Option<String>,
}

impl Args {
    fn resolve_graph_antialias(&self, configured: Option<bool>) -> bool {
        if self.graph_antialias {
            true
        } else if self.no_graph_antialias {
            false
        } else {
            configured.unwrap_or(false)
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, ValueEnum, Deserialize)]
#[serde(rename_all = "kebab-case")]
enum ImageProtocolType {
    Auto,
    Iterm,
    Kitty,
    KittyUnicode,
}

impl From<Option<ImageProtocolType>> for protocol::ImageProtocol {
    fn from(protocol: Option<ImageProtocolType>) -> Self {
        match protocol {
            Some(ImageProtocolType::Auto) => protocol::auto_detect(),
            Some(ImageProtocolType::Iterm) => protocol::ImageProtocol::Iterm2,
            Some(ImageProtocolType::Kitty) => protocol::ImageProtocol::Kitty,
            Some(ImageProtocolType::KittyUnicode) => protocol::ImageProtocol::KittyUnicode {
                tmux: protocol::detect_tmux(),
            },
            None => protocol::auto_detect(),
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, ValueEnum, Deserialize)]
#[serde(rename_all = "lowercase")]
enum CommitOrderType {
    Chrono,
    Topo,
}

impl From<Option<CommitOrderType>> for git::SortCommit {
    fn from(order: Option<CommitOrderType>) -> Self {
        match order {
            Some(CommitOrderType::Chrono) => git::SortCommit::Chronological,
            Some(CommitOrderType::Topo) => git::SortCommit::Topological,
            None => git::SortCommit::Chronological,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, ValueEnum, Deserialize)]
#[serde(rename_all = "lowercase")]
enum GraphWidthType {
    Auto,
    Double,
    Single,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, ValueEnum, Deserialize)]
#[serde(rename_all = "lowercase")]
enum GraphStyle {
    Rounded,
    Angular,
    Curved,
}

impl From<Option<GraphStyle>> for graph::GraphStyle {
    fn from(style: Option<GraphStyle>) -> Self {
        match style {
            Some(GraphStyle::Rounded) => graph::GraphStyle::Rounded,
            Some(GraphStyle::Angular) => graph::GraphStyle::Angular,
            Some(GraphStyle::Curved) => graph::GraphStyle::Curved,
            None => graph::GraphStyle::Rounded,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, ValueEnum, Deserialize)]
#[serde(rename_all = "lowercase")]
enum InitialSelection {
    Latest,
    Head,
}

impl From<Option<InitialSelection>> for app::InitialSelection {
    fn from(selection: Option<InitialSelection>) -> Self {
        match selection {
            Some(InitialSelection::Latest) => app::InitialSelection::Latest,
            Some(InitialSelection::Head) => app::InitialSelection::Head,
            None => app::InitialSelection::Latest,
        }
    }
}

type Result<T> = std::result::Result<T, Box<dyn std::error::Error>>;

fn main() -> Result<()> {
    let args = Args::parse();
    let (core_config, ui_config, graph_config, color_theme, keybind_patch) = config::load()?;
    let keybind = keybind::KeyBind::new(keybind_patch);

    let max_count = args.max_count;
    let image_protocol = args.protocol.or(core_config.option.protocol).into();
    let order = args.order.or(core_config.option.order).into();
    let graph_width = args.graph_width.or(core_config.option.graph_width);
    let graph_style = args.graph_style.or(core_config.option.graph_style).into();
    let graph_antialias = args.resolve_graph_antialias(core_config.option.graph_antialias);
    let graph_image_width_mode = graph_config.row_image_width;
    let initial_selection = args
        .initial_selection
        .or(core_config.option.initial_selection)
        .into();
    let mailmap = core_config.git.mailmap;
    let primary_branch = args.primary_branch.filter(|s| !s.is_empty());

    let graph_color_set = color::GraphColorSet::new(&graph_config.color);

    let ctx = Rc::new(app::AppContext {
        keybind,
        core_config,
        ui_config,
        color_theme,
        image_protocol,
    });

    let ec = event::EventController::init();
    let mut refresh_view_context = None;
    let mut refresh_message: Option<String> = None;
    let mut refresh_warning: Option<String> = None;
    // the graph column width chosen while running; it outlives refreshes
    let mut chosen_graph_width: Option<graph::GraphWidth> = None;
    // the number of graph lanes chosen in an earlier run
    let saved_graph_lanes = git::saved_graph_lanes(Path::new("."));
    let mut terminal = None;

    let ret: Result<()> = (|| loop {
        let repository = git::Repository::load(Path::new("."), order, max_count, mailmap)?;

        let graph = graph::calc_graph(&repository, primary_branch.as_deref());

        // Changing the graph column width redraws the graph without reloading the repository.
        loop {
            let graph_width = match chosen_graph_width {
                Some(chosen) => check::fit_graph_width(&graph, chosen)?,
                None => {
                    let cell_width_type = check::decide_cell_width_type(&graph, graph_width)?;
                    let width = graph::GraphWidth {
                        max_lanes: saved_graph_lanes,
                        ..graph::GraphWidth::new(cell_width_type)
                    };
                    chosen_graph_width = saved_graph_lanes.map(|_| width);
                    check::fit_graph_width(&graph, width)?
                }
            };

            let graph_image_manager = GraphImageManager::new(
                &graph,
                &graph_color_set,
                graph_width.cell_width_type,
                graph_style,
                graph_antialias,
                graph_image_width_mode,
                image_protocol,
            )
            .with_max_lanes(graph_width.max_lanes);

            if terminal.is_none() {
                terminal = Some(ratatui::init());
            }

            let mut app = App::new(
                &repository,
                graph_image_manager,
                &graph_color_set,
                initial_selection,
                ctx.clone(),
                &ec,
            );

            if let Some(message) = refresh_message.take() {
                app.success_notification(message);
            }
            if let Some(warning) = refresh_warning.take() {
                app.warn_notification(warning);
            }

            match app.run(terminal.as_mut().unwrap(), refresh_view_context)? {
                Ret::Quit => {
                    return Ok(());
                }
                Ret::Refresh(request) => {
                    refresh_view_context = Some(request.context);
                    refresh_message = request.message;
                    match request.graph_width {
                        Some(width) => {
                            chosen_graph_width = Some(width);
                            let saved = git::save_graph_lanes(Path::new("."), width.max_lanes);
                            if let Err(err) = saved {
                                refresh_warning = Some(format!("Graph width not saved: {err}"));
                            }
                        }
                        None => break,
                    }
                }
            }
        }
    })();

    if terminal.is_some() {
        ratatui::restore();
    }
    ret
}
