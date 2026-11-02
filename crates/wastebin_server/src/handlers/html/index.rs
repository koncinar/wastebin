use askama::Template;
use askama_web::WebTemplate;
use axum::extract::{Query, State};
use serde::Deserialize;

use crate::handlers::extract::Theme;
use crate::handlers::html::{ErrorResponse, make_error};
use crate::i18n::Lang;
use crate::{Database, Highlighter, Page};
use wastebin_core::db::read::ListEntry;

pub const PASTES_PER_PAGE: usize = 20;

#[derive(Deserialize, Debug, Default)]
pub struct Pagination {
    pub page: Option<usize>,
}

/// GET handler for the index page.
pub async fn get(
    State(page): State<Page>,
    State(db): State<Database>,
    State(highlighter): State<Highlighter>,
    Query(pagination): Query<Pagination>,
    theme: Option<Theme>,
    lang: Lang,
) -> Result<Index, ErrorResponse> {
    let requested_page = pagination.page.unwrap_or(1).max(1);

    let list_page = db
        .list_paginated(requested_page, PASTES_PER_PAGE)
        .await
        .map_err(|err| make_error(err.into(), page.clone(), theme.clone(), lang))?;

    let total_pages = list_page.total.div_ceil(PASTES_PER_PAGE).max(1);
    let current_page = requested_page.min(total_pages);

    let entries = if current_page == requested_page {
        list_page.entries
    } else {
        db.list_paginated(current_page, PASTES_PER_PAGE)
            .await
            .map_err(|err| make_error(err.into(), page.clone(), theme.clone(), lang))?
            .entries
    };

    Ok(Index {
        page,
        theme,
        lang,
        highlighter,
        entries,
        current_page,
        total_pages,
        total_entries: list_page.total,
    })
}

/// Index page displaying a form for paste insertion, a selection box for languages, and a
/// chronologically ordered paginated list of pastes.
#[derive(Template, WebTemplate)]
#[template(path = "index.html")]
pub(crate) struct Index {
    page: Page,
    theme: Option<Theme>,
    lang: Lang,
    highlighter: Highlighter,
    entries: Vec<ListEntry>,
    current_page: usize,
    total_pages: usize,
    total_entries: usize,
}

#[cfg(test)]
mod tests {
    use crate::cache::valid_extension;
    use crate::handlers::insert::form::Entry;
    use crate::test_helpers::{Client, StoreCookies};
    use reqwest::StatusCode;

    /// Picker extension coverage.
    ///
    /// The template submits `extensions.first()`.
    #[test]
    fn picker_extensions_pass_validation() {
        let highlighter = wastebin_highlight::Highlighter::default();

        for syntax in highlighter.syntaxes() {
            let Some(extension) = syntax.extensions.first() else {
                continue;
            };

            assert!(
                valid_extension(extension),
                "syntax {:?} submits invalid extension {:?}",
                syntax.name,
                extension
            );
        }
    }

    #[tokio::test]
    async fn index_shows_paginated_pastes() -> Result<(), Box<dyn std::error::Error>> {
        let client = Client::new(StoreCookies(false)).await;

        for i in 1..=25 {
            let data = Entry {
                text: format!("body {i}"),
                title: format!("PasteItem{i:02}"),
                extension: Some(String::from("rs")),
                ..Default::default()
            };
            let res = client.post_form().form(&data).send().await?;
            assert_eq!(res.status(), StatusCode::SEE_OTHER);
        }

        let res = client.get("/").send().await?;
        assert_eq!(res.status(), StatusCode::OK);
        let body = res.text().await?;

        // Page 1 has newest 20 (PasteItem25 down to PasteItem06)
        assert!(body.contains("PasteItem25"));
        assert!(body.contains("PasteItem06"));
        assert!(!body.contains("PasteItem05"));
        assert!(body.contains("page 1 / 2"));

        // Page 2 has oldest 5 (PasteItem05 down to PasteItem01)
        let res2 = client.get("/?page=2").send().await?;
        assert_eq!(res2.status(), StatusCode::OK);
        let body2 = res2.text().await?;
        assert!(body2.contains("PasteItem05"));
        assert!(body2.contains("PasteItem01"));
        assert!(!body2.contains("PasteItem06"));
        assert!(body2.contains("page 2 / 2"));

        Ok(())
    }
}
