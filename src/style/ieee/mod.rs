//! Consolidated IEEE bibliography style as defined in the
//! [2018 IEEE Reference Guide](https://ieeeauthorcenter.ieee.org/wp-content/uploads/IEEE-Reference-Guide.pdf)
//! and the document
//! ["How to Cite References: The IEEE Citation Style"](https://ieee-dataport.org/sites/default/files/analysis/27/IEEE%20Citation%20Guidelines.pdf).

mod abbreviations;

use isolang::Language;

use super::{
    format_range, name_list_straight, push_comma_quote_aware, BibliographyFormatter,
    DisplayString, Formatting,
};
use crate::lang::{en, SentenceCase, TitleCase};
use crate::types::EntryType::*;
use crate::types::{Date, NumOrStr, PersonRole};
use crate::Entry;

/// Generator for the IEEE reference list.
#[derive(Clone, Debug)]
pub struct Ieee {
    sentence_case: SentenceCase,
    title_case: TitleCase,
    et_al_threshold: Option<u32>,
}

fn get_canonical_parent(entry: &Entry) -> Option<&Entry> {
    let section = select!((Chapter | Scene | Web) > ("p":*));
    let anthology = select!(Anthos > ("p": Anthology));
    let entry_spec = select!(Entry > ("p":(Reference | Repository)));
    let proceedings = select!(* > ("p":(Conference | Proceedings)));

    section
        .apply(entry)
        .or_else(|| anthology.apply(entry))
        .or_else(|| entry_spec.apply(entry))
        .or_else(|| proceedings.apply(entry))
        .and_then(|mut bindings| bindings.remove("p"))
}

impl Ieee {
    /// Creates a new IEEE bibliography generator.
    pub fn new() -> Self {
        let mut title_case = TitleCase::default();
        title_case.always_capitalize_min_len = Some(4);
        Self {
            sentence_case: SentenceCase::default(),
            title_case,
            et_al_threshold: Some(6),
        }
    }

    fn and_list(&self, names: Vec<String>) -> String {
        let name_len = names.len() as u32;
        let mut res = String::new();
        let threshold = self.et_al_threshold.unwrap_or(0);

        for (index, name) in names.into_iter().enumerate() {
            if threshold > 0 && index > 1 && name_len >= threshold {
                break;
            }

            res += &name;

            if (index as i32) <= name_len as i32 - 2 {
                res += ", ";
            }
            if (index as i32) == name_len as i32 - 2 {
                res += "and ";
            }
        }

        if threshold > 0 && name_len >= threshold {
            res += "et al."
        }

        res
    }

    fn show_url(&self, entry: &Entry) -> bool {
        entry.any_url().is_some()
    }

    fn get_author(&self, entry: &Entry, canonical: &Entry) -> String {
        #[derive(Clone, Debug)]
        enum AuthorRole {
            Normal,
            Director,
            ExecutiveProducer,
        }

        impl Default for AuthorRole {
            fn default() -> Self {
                Self::Normal
            }
        }

        let mut names = None;
        let mut role = AuthorRole::default();
        if entry.entry_type == Video {
            let tv_series = select!((Video["issue", "volume"]) > Video);
            let dirs = entry.affiliated_filtered(PersonRole::Director);

            if tv_series.matches(entry) {
                // TV episode
                let mut dir_name_list_straight = name_list_straight(&dirs)
                    .into_iter()
                    .map(|s| format!("{} (Director)", s))
                    .collect::<Vec<String>>();

                let writers = entry.affiliated_filtered(PersonRole::Writer);
                let mut writers_name_list_straight = name_list_straight(&writers)
                    .into_iter()
                    .map(|s| format!("{} (Writer)", s))
                    .collect::<Vec<String>>();
                dir_name_list_straight.append(&mut writers_name_list_straight);

                if !dirs.is_empty() {
                    names = Some(dir_name_list_straight);
                }
            } else {
                // Film
                if !dirs.is_empty() {
                    names = Some(name_list_straight(&dirs));
                    role = AuthorRole::Director;
                } else {
                    // TV show
                    let prods = entry.affiliated_filtered(PersonRole::ExecutiveProducer);

                    if !prods.is_empty() {
                        names = Some(name_list_straight(&prods));
                        role = AuthorRole::ExecutiveProducer;
                    }
                }
            }
        }

        let authors = names.or_else(|| {
            entry
                .authors_fallible()
                .or_else(|| canonical.authors_fallible())
                .map(|n| name_list_straight(n))
        });
        let al = if let Some(authors) = authors {
            let count = authors.len();
            let amps = self.and_list(authors);
            match role {
                AuthorRole::Normal => amps,
                AuthorRole::ExecutiveProducer if count == 1 => {
                    format!("{}, Executive Prod", amps)
                }
                AuthorRole::ExecutiveProducer => format!("{}, Executive Prods", amps),
                AuthorRole::Director if count == 1 => format!("{}, Director", amps),
                AuthorRole::Director => format!("{}, Directors", amps),
            }
        } else if let Some(eds) = entry.editors() {
            if !eds.is_empty() {
                format!(
                    "{}, {}",
                    self.and_list(name_list_straight(&eds)),
                    if eds.len() == 1 { "Ed." } else { "Eds." }
                )
            } else {
                String::new()
            }
        } else {
            String::new()
        };

        al
    }

    fn get_title_element(&self, entry: &Entry, canonical: &Entry) -> DisplayString {
        // Article > Periodical: "<SC>," _<abbr(TC)>_
        // Any > Conference:     <SC>. Presented at <abbr(TC)>
        // Any > Anthology:      "<SC>," in _<TC>_ (TC, no. <issue>)
        // entry != canonical:   "<SC>," in _<TC>_
        // Legislation:          _<serial number>, <TC>_
        // Repository, Video, Reference, Book, Proceedings, Anthology, : _<TC>_
        // Fallback:             "<SC>,"

        let mut res = DisplayString::new();

        if entry != canonical {
            let entry_title = entry.title_fmt(None, Some(&self.sentence_case));
            let canon_title = canonical.title_fmt(Some(&self.title_case), None);

            if let Some(et) = entry_title {
                if canonical.entry_type == Conference {
                    res += &et.value.sentence_case;
                    res.push('.');
                } else {
                    res += "“";
                    res += &et.value.sentence_case;
                    res += ",”";
                }

                if canon_title.is_some() {
                    res.push(' ');
                }
            }

            if let Some(ct) = canon_title {
                let ct = abbreviations::abbreviate_journal(&ct.value.title_case);

                if canonical.entry_type == Conference {
                    res += "Presented at ";
                    res += &ct;
                } else {
                    if let Some(lang) = entry.language().or_else(|| canonical.language())
                    {
                        res += "(in ";
                        res += Language::from_639_1(lang.language.as_str())
                            .unwrap()
                            .to_name();
                        res += ") ";
                    }

                    if entry.entry_type != Article || canonical.entry_type != Periodical {
                        res += "in ";
                    }
                    res.start_format(Formatting::Italic);
                    res += &ct;
                    res.commit_formats();

                    // Do the series parentheses thing here
                    let spec = select!(Anthology > ("p":(Anthology["title"])));
                    if let Some(mut bindings) = spec.apply(canonical) {
                        let parenth_anth = bindings.remove("p").unwrap();
                        let parenth_title =
                            parenth_anth.title_fmt(Some(&self.title_case), None).unwrap();
                        res += " (";
                        res += &parenth_title.value.title_case;

                        res.add_if_some(
                            parenth_anth.issue().map(|i| i.to_string()),
                            Some(", no. "),
                            None,
                        );
                        res += ")";
                    }

                    // And the conference series thing as well
                    let spec =
                        select!(Proceedings > ("p":(Proceedings | Anthology | Misc)));
                    if let Some(mut bindings) = spec.apply(canonical) {
                        let par_conf = bindings.remove("p").unwrap();
                        if let Some(parenth_title) =
                            par_conf.title_fmt(Some(&self.title_case), None)
                        {
                            res += " in ";
                            res += &parenth_title.value.title_case;
                        }
                    }
                }
            }
        // No canonical parent
        } else if [
            Legislation,
            Repository,
            Video,
            Reference,
            Book,
            Proceedings,
            Anthology,
        ]
        .contains(&entry.entry_type)
        {
            res.start_format(Formatting::Italic);

            if entry.entry_type == Legislation {
                res.add_if_some(entry.serial_number(), None, None);
            }

            if let Some(title) = entry.title_fmt(Some(&self.title_case), None) {
                if !res.is_empty() {
                    res += ", ";
                }

                res += &title.value.title_case;
            }
            res.commit_formats();
        } else {
            if let Some(title) = entry.title_fmt(None, Some(&self.sentence_case)) {
                res += "“";
                res += &title.value.sentence_case;
                res += ",”";
            }
        }

        res
    }

    fn get_addons(
        &self,
        entry: &Entry,
        canonical: &Entry,
        chapter: Option<u32>,
        section: Option<u32>,
    ) -> Vec<String> {
        let mut res = vec![];
        let preprint =
            select!((Article | Book | Anthos) > ("p": Repository)).apply(entry);
        let web_parented = select!(* > ("p":(Blog | Web))).apply(entry);

        match (entry.entry_type, canonical.entry_type) {
            (_, Conference) | (_, Proceedings) => {
                if canonical.entry_type == Proceedings {
                    if let Some(eds) = canonical.editors() {
                        let mut al = self.and_list(name_list_straight(&eds));
                        if eds.len() > 1 {
                            al += ", Eds."
                        } else {
                            al += ", Ed."
                        }
                        res.push(al);
                    }

                    if let Some(vols) = entry.volume().or_else(|| canonical.volume()) {
                        res.push(format_range("vol.", "vols.", &vols));
                    }

                    if let Some(ed) = canonical.edition() {
                        match ed {
                            NumOrStr::Number(i) => {
                                if *i > 1 {
                                    res.push(format!("{} ed.", en::get_ordinal(*i)));
                                }
                            }
                            NumOrStr::Str(s) => res.push(s.into()),
                        }
                    }
                }

                if let Some(location) = canonical.location() {
                    res.push(location.into());
                }

                if canonical.entry_type != Conference || !self.show_url(entry) {
                    if let Some(date) = entry.any_date() {
                        if let Some(month) = date.month {
                            res.push(if let Some(day) = date.day {
                                format!(
                                    "{} {}",
                                    en::get_month_abbr(month, true).unwrap(),
                                    day + 1
                                )
                            } else {
                                en::get_month_abbr(month, true).unwrap()
                            });
                        }

                        res.push(date.display_year());
                    }
                }

                if canonical.entry_type == Conference {
                    if let Some(sn) = entry.serial_number() {
                        res.push(format!("Paper {}", sn));
                    }
                } else {
                    if let Some(pages) = entry.page_range() {
                        res.push(format_range("p.", "pp.", &pages));
                    }

                    if let Some(doi) = entry.doi() {
                        res.push(format!("doi: {}", doi));
                    }
                }
            }
            (_, Reference) => {
                let has_url = self.show_url(entry);
                let date = entry.any_date().map(|date| {
                    let mut res = if let Some(month) = date.month {
                        if let Some(day) = date.day {
                            format!(
                                "{} {}, ",
                                en::get_month_abbr(month, true).unwrap(),
                                day + 1
                            )
                        } else {
                            format!("{} ", en::get_month_abbr(month, true).unwrap())
                        }
                    } else {
                        String::new()
                    };

                    res += &date.display_year();
                    res
                });

                if let Some(ed) = canonical.edition() {
                    match ed {
                        NumOrStr::Number(i) => {
                            if *i > 1 {
                                res.push(format!("{} ed.", en::get_ordinal(*i)));
                            }
                        }
                        NumOrStr::Str(s) => res.push(s.clone()),
                    }
                }

                if !has_url {
                    if let Some(publisher) =
                        canonical.organization().or_else(|| canonical.publisher())
                    {
                        res.push(publisher.into());

                        if let Some(location) = canonical.location() {
                            res.push(location.into());
                        }
                    }

                    if let Some(date) = date {
                        res.push(date);
                    }

                    if let Some(pages) = entry.page_range() {
                        res.push(format_range("p.", "pp.", &pages));
                    }
                } else {
                    if let Some(date) = date {
                        res.push(format!("({})", date));
                    }
                }
            }
            (_, Repository) => {
                if let Some(sn) = canonical.serial_number() {
                    res.push(format!("(version {})", sn));
                } else if let Some(date) = canonical.date().or_else(|| entry.any_date()) {
                    res.push(format!("({})", date.year));
                }

                if let Some(publisher) =
                    canonical.publisher().or_else(|| canonical.organization())
                {
                    let mut publ = String::new();
                    if let Some(location) = canonical.location() {
                        publ += location;
                        publ += ": ";
                    }

                    publ += publisher;

                    if let Some(lang) = entry.language().or_else(|| canonical.language())
                    {
                        publ += " (in ";
                        publ += Language::from_639_1(lang.language.as_str())
                            .unwrap()
                            .to_name();
                        publ.push(')');
                    }

                    res.push(publ);
                }
            }
            (_, Video) => {
                if let Some(date) = canonical.date().or_else(|| entry.any_date()) {
                    res.push(format!("({})", date.year));
                }
            }
            (_, Patent) => {
                let mut start = String::new();
                if let Some(location) = canonical.location() {
                    start += location;
                    start.push(' ');
                }

                start += "Patent";

                if let Some(sn) = canonical.serial_number() {
                    start += &format!(" {}", sn);
                }

                if self.show_url(entry) {
                    let mut fin = String::new();
                    if let Some(date) = entry.any_date() {
                        fin += "(";
                        fin += &date.display_year();
                        if let Some(month) = date.month {
                            fin += ", ";
                            fin += &(if let Some(day) = date.day {
                                format!(
                                    "{} {}",
                                    en::get_month_abbr(month, true).unwrap(),
                                    day + 1
                                )
                            } else {
                                en::get_month_abbr(month, true).unwrap()
                            });
                        }
                        fin += "). ";
                    }

                    fin += &start;

                    res.push(fin);
                } else {
                    res.push(start);

                    if let Some(date) = entry.any_date() {
                        if let Some(month) = date.month {
                            res.push(if let Some(day) = date.day {
                                format!(
                                    "{} {}",
                                    en::get_month_abbr(month, true).unwrap(),
                                    day + 1
                                )
                            } else {
                                en::get_month_abbr(month, true).unwrap()
                            });
                        }

                        res.push(date.display_year());
                    }
                }
            }
            (_, Periodical) => {
                if let Some(vols) = canonical.volume() {
                    res.push(format_range("vol.", "vols.", &vols));
                }

                if let Some(iss) = canonical.issue() {
                    res.push(format!("no. {}", iss));
                }

                let pages = if let Some(pages) = entry.page_range() {
                    res.push(format_range("p.", "pp.", &pages));
                    true
                } else {
                    false
                };

                if let Some(date) = entry.any_date() {
                    if let Some(month) = date.month {
                        res.push(if let Some(day) = date.day {
                            format!(
                                "{} {}",
                                en::get_month_abbr(month, true).unwrap(),
                                day + 1
                            )
                        } else {
                            en::get_month_abbr(month, true).unwrap()
                        });
                    }

                    res.push(date.display_year());
                }

                if !pages {
                    if let Some(sn) = entry.serial_number() {
                        res.push(format!("Art. no. {}", sn));
                    }
                }

                if let Some(doi) = entry.doi() {
                    res.push(format!("doi: {}", doi));
                }
            }
            (_, Report) => {
                if let Some(publisher) =
                    canonical.organization().or_else(|| canonical.publisher())
                {
                    res.push(publisher.into());

                    if let Some(location) = canonical.location() {
                        res.push(location.into());
                    }
                }

                if let Some(sn) = canonical.serial_number() {
                    res.push(format!("Rep. {}", sn));
                }

                let date = entry.any_date().map(|date| {
                    let mut res = if let Some(month) = date.month {
                        if let Some(day) = date.day {
                            format!(
                                "{} {}, ",
                                en::get_month_abbr(month, true).unwrap(),
                                day + 1
                            )
                        } else {
                            format!("{} ", en::get_month_abbr(month, true).unwrap())
                        }
                    } else {
                        String::new()
                    };

                    res += &date.display_year();
                    res
                });

                if !self.show_url(entry) {
                    if let Some(date) = date.clone() {
                        res.push(date);
                    }
                }

                if let Some(vols) = canonical.volume().or_else(|| entry.volume()) {
                    res.push(format_range("vol.", "vols.", &vols));
                }


                if let Some(iss) = canonical.issue() {
                    res.push(format!("no. {}", iss));
                }


                if self.show_url(entry) {
                    if let Some(date) = date {
                        res.push(date);
                    }
                }
            }
            (_, Thesis) => {
                res.push("Thesis".to_string());
                if let Some(org) = canonical.organization() {
                    res.push(abbreviations::abbreviate_journal(&org));

                    if let Some(location) = canonical.location() {
                        res.push(location.into());
                    }
                }

                if let Some(sn) = entry.serial_number() {
                    res.push(sn.into());
                }

                if let Some(date) = entry.any_date() {
                    res.push(date.display_year());
                }
            }
            (_, Legislation) => {}
            (_, Manuscript) => {
                res.push("unpublished".to_string());
            }
            _ if preprint.is_some() => {
                let parent = preprint.unwrap().remove("p").unwrap();
                if let Some(sn) = entry.serial_number() {
                    let mut sn = if let Some(url) = entry.any_url() {
                        if !sn.to_lowercase().contains("arxiv")
                            && (url.value.host_str().unwrap_or("").to_lowercase()
                                == "arxiv.org"
                                || parent
                                    .title()
                                    .map(|e| e.to_lowercase())
                                    .unwrap_or_default()
                                    == "arxiv")
                        {
                            format!("arXiv: {}", sn)
                        } else {
                            sn.to_string()
                        }
                    } else {
                        sn.to_string()
                    };

                    if let Some(al) = entry.archive().or_else(|| parent.archive()) {
                        sn += " [";
                        sn += al;
                        sn += "]";
                    }

                    res.push(sn);
                }

                if let Some(date) = entry.any_date() {
                    if let Some(month) = date.month {
                        res.push(if let Some(day) = date.day {
                            format!(
                                "{} {}",
                                en::get_month_abbr(month, true).unwrap(),
                                day + 1
                            )
                        } else {
                            en::get_month_abbr(month, true).unwrap()
                        });
                    }

                    res.push(date.display_year());
                }
            }
            (Web, _) | (Blog, _) => {
                if let Some(publisher) =
                    entry.publisher().or_else(|| entry.organization())
                {
                    res.push(publisher.into());
                }
            }
            _ if web_parented.is_some() => {
                let parent = web_parented.unwrap().remove("p").unwrap();
                if let Some(publisher) = parent
                    .title()
                    .or_else(|| parent.publisher())
                    .or_else(|| entry.publisher())
                    .or_else(|| parent.organization())
                    .or_else(|| entry.organization())
                {
                    res.push(publisher.into());
                }
            }
            _ => {
                if let (Some(_), Some(eds)) = (
                    entry.authors().get(0),
                    entry.editors().or_else(|| canonical.editors()),
                ) {
                    let mut al = self.and_list(name_list_straight(&eds));
                    if eds.len() > 1 {
                        al += ", Eds."
                    } else {
                        al += ", Ed."
                    }
                    res.push(al);
                }

                if let Some(vols) = entry.volume().or_else(|| canonical.volume()) {
                    res.push(format_range("vol.", "vols.", &vols));
                }

                if let Some(ed) = canonical.edition() {
                    match ed {
                        NumOrStr::Number(i) => {
                            if *i > 1 {
                                res.push(format!("{} ed.", en::get_ordinal(*i)));
                            }
                        }
                        NumOrStr::Str(s) => res.push(s.clone()),
                    }
                }

                if let Some(publisher) =
                    canonical.publisher().or_else(|| canonical.organization())
                {
                    let mut publ = String::new();
                    if let Some(location) = canonical.location() {
                        publ += location;
                        publ += ": ";
                    }

                    publ += &publisher;

                    if let Some(lang) = entry.language().or_else(|| canonical.language())
                    {
                        publ += " (in ";
                        publ += Language::from_639_1(lang.language.as_str())
                            .unwrap()
                            .to_name();
                        publ.push(')');
                    }

                    res.push(publ);
                }

                if let Some(date) = canonical.any_date() {
                    res.push(date.display_year());
                }

                if let Some(chapter) = chapter {
                    res.push(format!("ch. {}", chapter));
                }

                if let Some(section) = section {
                    res.push(format!("sec. {}", section));
                }

                if let Some(pages) = entry.page_range() {
                    res.push(format_range("p.", "pp.", &pages));
                }
            }
        }

        res
    }

    fn formt_date(&self, date: &Date) -> String {
        let mut res = String::new();
        if let Some(month) = date.month {
            res += &(if let Some(day) = date.day {
                format!("{} {},", en::get_month_abbr(month, true).unwrap(), day + 1)
            } else {
                en::get_month_abbr(month, true).unwrap()
            });
            res += " ";
        }

        res += &date.display_year();
        res
    }
}

impl BibliographyFormatter for Ieee {
    fn format(&self, mut entry: &Entry, _prev: Option<&Entry>) -> DisplayString {
        let mut parent = entry.parents().and_then(|v| v.first());
        let mut sn_stack = vec![];
        while entry.title().is_none() && select!(Chapter | Scene).matches(entry) {
            if let Some(sn) = entry.serial_number() {
                sn_stack.push(sn);
            }
            if let Some(p) = parent {
                entry = &p;
                parent = entry.parents().and_then(|v| v.first());
            } else {
                break;
            }
        }

        if entry.entry_type == Chapter {
            if let Some(sn) = entry.serial_number() {
                sn_stack.push(sn);
            }
        }

        let secs = sn_stack
            .into_iter()
            .map(|s| str::parse::<u32>(&s))
            .filter(|s| s.is_ok())
            .map(|s| s.unwrap())
            .collect::<Vec<_>>();

        let chapter = secs.get(0).map(|c| c.clone());
        let section = if secs.len() > 1 {
            secs.last().map(|c| c.clone())
        } else {
            None
        };

        let url = self.show_url(entry);

        let parent = get_canonical_parent(entry);
        let canonical = parent.unwrap_or(entry);

        let authors = self.get_author(entry, canonical);
        let title = self.get_title_element(entry, canonical);
        let addons = self.get_addons(entry, canonical, chapter, section);

        let mut res = DisplayString::from_string(authors);

        if canonical.entry_type == Legislation {
            if let Some(NumOrStr::Str(session)) = entry.edition() {
                if !res.is_empty() {
                    res += ". ";
                }
                res += session;
            }
        }

        if canonical.entry_type == Video {
            if let Some(location) = canonical.location() {
                if !res.is_empty() {
                    res += ", ";
                }
                res += location;
            }
        } else if canonical.entry_type == Legislation
            || ((canonical.entry_type == Conference || canonical.entry_type == Patent)
                && url)
        {
            if let Some(date) = entry.any_date() {
                if !res.is_empty() {
                    res += ". ";
                }
                res.push('(');
                res += &self.formt_date(&date);
                res.push(')');
            }
        }

        if !res.is_empty() && !title.is_empty() {
            if canonical.entry_type == Legislation
                || canonical.entry_type == Video
                || ((canonical.entry_type == Conference
                    || canonical.entry_type == Patent)
                    && url)
            {
                res += ". ";
            } else {
                res += ", ";
            }
        }
        res += title;

        let cur_len = res.len();
        if cur_len > 4
            && res.value.is_char_boundary(cur_len - 4)
            && &res.value[cur_len - 4 ..] == ",”"
        {
            if addons.is_empty() {
                res.value = (&res.value[.. cur_len - 4]).into();
                res.value += "”";
            } else {
                res.push(' ');
            }
        } else if !res.is_empty() && !addons.is_empty() {
            res += ", ";
        }

        let addon_count = addons.len();
        for (index, addon) in addons.into_iter().enumerate() {
            res += &addon;
            if index + 1 < addon_count {
                res += ", "
            }
        }

        push_comma_quote_aware(&mut res.value, '.', false);

        if url {
            if let Some(url) = entry.any_url() {
                if !res.is_empty() {
                    res += " ";
                }

                if canonical.entry_type != Web && canonical.entry_type != Blog {
                    if let Some(date) = &url.visit_date {
                        res += &format!("Accessed: {}. ", self.formt_date(&date));
                    }

                    if canonical.entry_type == Video {
                        res += "[Online Video]";
                    } else {
                        res += "[Online]";
                    }

                    res += ". Available: ";
                    res.start_format(Formatting::NoHyphenation);
                    res += url.value.as_str();
                    res.commit_formats();
                } else {
                    res.start_format(Formatting::NoHyphenation);
                    res += url.value.as_str();
                    res.commit_formats();

                    if let Some(date) = &url.visit_date {
                        res += &format!(" (accessed: {}).", self.formt_date(&date));
                    }
                }
            }
        }

        if let Some(note) = entry.note() {
            if !res.is_empty() {
                res += " ";
            }

            res += &format!("({})", note);
        }

        res
    }
}
