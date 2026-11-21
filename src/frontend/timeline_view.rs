use chrono::{Days, NaiveDate};

use crate::backend::{collect_timeline, Project};
use crate::frontend::UiAction;

pub(crate) fn show(ui: &mut egui::Ui, project: &Project) -> UiAction {
    ui.heading(format!("Timeline — {}", project.meta.name));
    if let Some(date) = project.meta.start_date {
        ui.label(format!("Day 0 is {date}"));
    }
    ui.label("Foundings before Day 0 stay off the timeline.");
    let entries = collect_timeline(project.pages().iter().map(|page| &page.frontmatter));
    if entries.is_empty() {
        ui.label("No timeline entries yet.");
        return UiAction::None;
    }
    let mut action = UiAction::None;
    let mut previous: Option<Option<i32>> = None;
    egui::ScrollArea::vertical()
        .id_salt("timeline_scroll")
        .show(ui, |ui| {
            for entry in &entries {
                if previous != Some(entry.day) {
                    ui.heading(format_day(project.meta.start_date, entry.day));
                    previous = Some(entry.day);
                }
                ui.horizontal(|ui| {
                    if ui.button(&entry.title).clicked() {
                        action = UiAction::OpenPage(entry.page_id);
                    }
                    ui.label(&entry.detail);
                });
            }
        });
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
