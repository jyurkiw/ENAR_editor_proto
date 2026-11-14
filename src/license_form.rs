//! A license file's page (`<license>`, license-template-schema.xsd): its tracking
//! attributes and its notice paragraphs, where the `<date/>` and `<ownedBy/>` blanks show
//! as chips (the supplement that prints the license fills them in).

use leptos::prelude::*;
use shared::statblock::{self as sb, El, ROOT};

use crate::prose_editor::ChipEditor;
use crate::state::DocTab;
use crate::statblock_form::{card_bar, keys, text_in, Card, Loc, Rows};

/// One paragraph: a chip box.
pub(crate) fn p_row(c: Card, l: Loc, _i: usize) -> AnyView {
    let get = Signal::derive(move || c.m().at(&l.s[..l.n]).map(|e| e.text.clone()).unwrap_or_default());
    let put = Callback::new(move |v: String| c.at(l, |e| e.text = v));
    view! { <ChipEditor value=get set=put placeholder="(empty paragraph)"/> }.into_any()
}

#[component]
pub fn LicensePage(tab: DocTab) -> impl IntoView {
    let el = Memo::new(move |_| sb::read(&tab.doc().source, ROOT).filter(|m| m.tag == "license"));
    let c = Card::new(tab, ROOT, el, None, None);
    let r = Loc::ROOT;
    view! {
        {move || if el.with(|e| e.is_none()) {
            view! { <div class="empty"><p>"This license file can't be read."</p></div> }.into_any()
        } else {
            view! {
                <div class="sb-page">
                    <div class="table-card" class:dirty=move || c.d.dirty() class:failed=move || c.d.failed() on:keydown=keys(c)>
                        <div class="table-card-title">"License"
                            <span class="hint">"The notice text a supplement prints with <license file=…>. Nothing prints the attributes; they're for tracking."</span>
                        </div>
                        <div class="field-row">
                            {text_in(c, r, "name", "Name", "DMs Guild Community Content Agreement", "Required")}
                            {text_in(c, r, "version", "Version", "(none)", "The licensor's own version number")}
                        </div>
                        <div class="field-row">
                            {text_in(c, r, "download_date", "Copied on", "2026-11-06", "YYYY-MM-DD, YYYY-MM or YYYY")}
                            {text_in(c, r, "publication_date", "Published on", "(none)", "When the licensor published this version")}
                        </div>
                        <Rows c loc=r tag="p" title="Notice" min=1
                            hint="Written exactly as the license asks. \u{2039}date\u{203a} and \u{2039}owner\u{203a} chips are the blanks the supplement's <license date ownedBy> fills (\u{ff0b} or Ctrl+K inserts them)."
                            make=|_| El::new("p") row=p_row/>
                        {card_bar(c)}
                    </div>
                </div>
            }.into_any()
        }}
    }
}
