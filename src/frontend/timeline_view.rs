use chrono::{Days, NaiveDate};

use crate::backend::{collect_timeline, Project, TimelineEntry};
use crate::frontend::layout::{centered_column, CONTENT_WIDTH};
use crate::frontend::UiAction;

pub(crate) fn show(ui: &mut egui::Ui, project: &Project) -> UiAction {
    let entries = collect_timeline(project.pages().iter().map(|page| &page.frontmatter));
    egui::ScrollArea::vertical()
        .id_salt("timeline_scroll")
        .auto_shrink([false, false])
        .show(ui, |ui| {
            centered_column(ui, CONTENT_WIDTH, |ui| body(ui, project, &entries))
        })
        .inner
}

fn body(ui: &mut egui::Ui, project: &Project, entries: &[TimelineEntry]) -> UiAction {
    ui.heading(format!("Timeline — {}", project.meta.name));
    if let Some(date) = project.meta.start_date {
        ui.label(egui::RichText::new(format!("Day 0 is {date}")).weak());
    }
    ui.label(egui::RichText::new("Foundings before Day 0 stay off the timeline.").weak());
    ui.separator();
    if entries.is_empty() {
        ui.label(egui::RichText::new("No timeline entries yet.").weak());
        return UiAction::None;
    }
    let mut action = UiAction::None;
    let mut previous: Option<Option<i32>> = None;
    for entry in entries {
        if previous != Some(entry.day) {
            ui.add_space(10.0);
            ui.label(
                egui::RichText::new(format_day(project.meta.start_date, entry.day))
                    .size(18.0)
                    .strong(),
            );
            previous = Some(entry.day);
        }
        ui.horizontal(|ui| {
            if ui.button(&entry.title).clicked() {
                action = UiAction::OpenPage(entry.page_id);
            }
            ui.label(egui::RichText::new(&entry.detail).weak());
        });
    }
    action
}

fn format_day(start: Option<NaiveDate>, day: Option<i32>) -> String {
    let Some(day) = day else {
        return "Undated".to_owned();
    };
    let Some(start) = start else {
        return format!("Day {day}");
    };
    match shift_date(start, day) {
        Some(date) => format!("Day {day} · {date}"),
        None => format!("Day {day}"),
    }
}

fn shift_date(date: NaiveDate, day: i32) -> Option<NaiveDate> {
    if day >= 0 {
        let days = u64::try_from(day).ok()?;
        date.checked_add_days(Days::new(days))
    } else {
        let days = u64::try_from(day.checked_neg()?).ok()?;
        date.checked_sub_days(Days::new(days))
    }
}
