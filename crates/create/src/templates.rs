//! Contributor-original starter layouts, drawn as editable text and vector artwork.
use super::{CreateError, add_page, set_title};
use pdfcraft_cos::{Dict, Document, Object};
use pdfcraft_fonts::{literal, win_ansi, wrap};

#[derive(Clone, Copy, Debug)]
pub struct Starter {
    pub id: &'static str,
    pub title: &'static str,
    pub category: &'static str,
    pub description: &'static str,
    pub pages: usize,
}

pub const STARTERS: &[Starter] = &[
    Starter {
        id: "business-card",
        title: "Business card",
        category: "Business",
        description: "A clean editable card. Letter uses 3.5 x 2 in; A4 uses 85 x 55 mm. Finished trim size; no bleed or sheet imposition.",
        pages: 1,
    },
    Starter { id: "blank", title: "Blank page", category: "Essentials", description: "An open canvas for your next document.", pages: 1 },
    Starter {
        id: "letter",
        title: "Business letter",
        category: "Business",
        description: "Clean letterhead, recipient details and a signature block.",
        pages: 1,
    },
    Starter {
        id: "invoice",
        title: "Invoice",
        category: "Business",
        description: "Itemized services, payment details and a clear totals table.",
        pages: 1,
    },
    Starter {
        id: "quotation",
        title: "Quotation",
        category: "Business",
        description: "Present a price estimate with scope and acceptance details.",
        pages: 1,
    },
    Starter { id: "proposal", title: "Proposal", category: "Business", description: "A cover, a project approach and a next-steps page.", pages: 3 },
    Starter {
        id: "project-brief",
        title: "Project brief",
        category: "Business",
        description: "Goals, audience, deliverables and success measures on one page.",
        pages: 1,
    },
    Starter {
        id: "agenda",
        title: "Meeting agenda",
        category: "Business",
        description: "A timed agenda with clear outcomes and preparation notes.",
        pages: 1,
    },
    Starter {
        id: "minutes",
        title: "Meeting minutes",
        category: "Business",
        description: "Record decisions, action owners and follow-up dates.",
        pages: 2,
    },
    Starter {
        id: "report",
        title: "Business report",
        category: "Business",
        description: "A refined cover with findings and recommendations.",
        pages: 3,
    },
    Starter {
        id: "article",
        title: "Editorial article",
        category: "Writing",
        description: "A serif headline and readable two-column editorial pages.",
        pages: 3,
    },
    Starter {
        id: "booklet",
        title: "Booklet",
        category: "Writing",
        description: "Eight small-format pages: cover, contents, chapters and back cover. Pages are in reading order.",
        pages: 8,
    },
    Starter {
        id: "essay",
        title: "Essay",
        category: "Writing",
        description: "A title, introductory argument, body sections and references.",
        pages: 3,
    },
    Starter {
        id: "manuscript",
        title: "Manuscript",
        category: "Writing",
        description: "A restrained chapter layout with generous, readable spacing.",
        pages: 4,
    },
    Starter {
        id: "flyer",
        title: "Flyer",
        category: "Marketing",
        description: "A bold headline, feature highlights and a prominent call to action.",
        pages: 1,
    },
    Starter {
        id: "poster",
        title: "Event poster",
        category: "Marketing",
        description: "Colorful typography with room for the date, venue and registration details.",
        pages: 1,
    },
    Starter {
        id: "brochure",
        title: "Tri-fold brochure",
        category: "Marketing",
        description: "Two landscape sides with three clearly separated panels. Review fold and print settings before production.",
        pages: 2,
    },
    Starter {
        id: "newsletter",
        title: "Newsletter",
        category: "Marketing",
        description: "A masthead, lead story and compact two-column updates.",
        pages: 2,
    },
    Starter {
        id: "product-sheet",
        title: "Product sheet",
        category: "Marketing",
        description: "An overview, feature list, specifications and contact details.",
        pages: 1,
    },
    Starter {
        id: "press-release",
        title: "Press release",
        category: "Marketing",
        description: "A publication-ready structure with a dateline, quote and media contact.",
        pages: 2,
    },
    Starter {
        id: "lesson-plan",
        title: "Lesson plan",
        category: "Education",
        description: "Learning objectives, materials, activities and assessment notes.",
        pages: 2,
    },
    Starter {
        id: "worksheet",
        title: "Worksheet",
        category: "Education",
        description: "Student details, prompts and ample space for written answers.",
        pages: 2,
    },
    Starter {
        id: "study-guide",
        title: "Study guide",
        category: "Education",
        description: "Key concepts, review questions and revision notes.",
        pages: 3,
    },
    Starter {
        id: "resume",
        title: "Resume",
        category: "Personal",
        description: "A modern sidebar for contact details and skills beside your experience.",
        pages: 1,
    },
    Starter {
        id: "cover-letter",
        title: "Cover letter",
        category: "Personal",
        description: "A concise introduction, evidence of fit and a thoughtful closing.",
        pages: 1,
    },
    Starter {
        id: "weekly-planner",
        title: "Weekly planner",
        category: "Planning",
        description: "Seven undated columns with space for weekly priorities.",
        pages: 1,
    },
    Starter {
        id: "monthly-planner",
        title: "Monthly planner",
        category: "Planning",
        description: "An undated six-row calendar grid. Add the month and dates yourself.",
        pages: 1,
    },
    Starter {
        id: "daily-planner",
        title: "Daily planner",
        category: "Planning",
        description: "Daily priorities, a schedule and a notes area.",
        pages: 1,
    },
    Starter {
        id: "checklist",
        title: "Checklist",
        category: "Planning",
        description: "A practical checklist with interactive checkboxes and space for task details.",
        pages: 1,
    },
    Starter {
        id: "budget",
        title: "Budget planner",
        category: "Planning",
        description: "Track planned and actual income, expenses and savings.",
        pages: 1,
    },
    Starter {
        id: "timeline",
        title: "Project timeline",
        category: "Planning",
        description: "Four project phases with milestones, owners and target dates.",
        pages: 1,
    },
    Starter {
        id: "intake-form",
        title: "Client intake",
        category: "Forms",
        description: "Fillable contact details and a multiline request field.",
        pages: 1,
    },
    Starter {
        id: "feedback",
        title: "Feedback form",
        category: "Forms",
        description: "Fillable feedback, suggestions and follow-up details.",
        pages: 1,
    },
    Starter {
        id: "order-form",
        title: "Order form",
        category: "Forms",
        description: "Fillable customer and order details with space for instructions.",
        pages: 1,
    },
    Starter {
        id: "attendance",
        title: "Attendance sheet",
        category: "Forms",
        description: "A clean session register with fillable participant names.",
        pages: 1,
    },
];

pub fn find(id: &str) -> Result<&'static Starter, CreateError> {
    STARTERS.iter().find(|t| t.id == id).ok_or_else(|| CreateError::Invalid(format!("Unknown starter template: {id}")))
}

pub fn dimensions(id: &str, paper: &str) -> Result<(f64, f64), CreateError> {
    find(id)?;
    let (w, h) = match paper {
        "letter" => (612.0, 792.0),
        "a4" => (595.28, 841.89),
        _ => return Err(CreateError::Invalid("Paper must be letter or a4".into())),
    };
    Ok(match id {
        "business-card" => {
            if paper == "a4" {
                (85.0 * 72.0 / 25.4, 55.0 * 72.0 / 25.4)
            } else {
                (252.0, 144.0)
            }
        }
        "booklet" => {
            if paper == "a4" {
                (419.53, 595.28)
            } else {
                (h / 2.0, w)
            }
        }
        "brochure" | "weekly-planner" | "monthly-planner" | "timeline" => (h, w),
        _ => (w, h),
    })
}

/// The engine installs these with the existing form authoring API.
pub struct StarterField {
    pub name: String,
    pub rect: [f64; 4],
    pub checkbox: bool,
    pub multiline: bool,
}

pub fn fields(id: &str, paper: &str) -> Result<Vec<StarterField>, CreateError> {
    let (w, h) = dimensions(id, paper)?;
    let labels: &[&str] = match id {
        "intake-form" => &["Full name", "Email", "Phone", "Organization", "How can we help?"],
        "feedback" => &["Name (optional)", "Email (optional)", "What worked well?", "What could improve?", "Additional comments"],
        "order-form" => &["Customer name", "Contact email", "Delivery address", "Items and quantities", "Special instructions"],
        _ => &[],
    };
    if id == "checklist" || id == "attendance" {
        return Ok((0..12)
            .map(|i| {
                let top = h - 172.0 - i as f64 * 40.0;
                StarterField {
                    name: format!("{} {}", if id == "checklist" { "Done" } else { "Participant" }, i + 1),
                    rect: if id == "checklist" { [48.0, top - 16.0, 64.0, top] } else { [88.0, top - 23.0, w - 48.0, top] },
                    checkbox: id == "checklist",
                    multiline: false,
                }
            })
            .collect());
    }
    Ok(labels
        .iter()
        .enumerate()
        .map(|(i, label)| {
            let top = h - 184.0 - i as f64 * 103.0;
            StarterField { name: (*label).into(), rect: [48.0, top - 54.0, w - 48.0, top], checkbox: false, multiline: i >= 2 }
        })
        .collect())
}

const INK: [f64; 3] = [0.10, 0.15, 0.21];
const ACCENT: [f64; 3] = [0.05, 0.40, 0.46];
const PALE: [f64; 3] = [0.92, 0.96, 0.96];
const MUTED: [f64; 3] = [0.40, 0.45, 0.50];
const BODY: &str = "Replace this paragraph with your own content. Lead with the information your reader needs, support it with clear details, and finish with a useful next step. Keep each section focused and leave enough space for the page to breathe.";

struct Canvas {
    bytes: Vec<u8>,
    w: f64,
    h: f64,
}
impl Canvas {
    fn rect(&mut self, x: f64, y: f64, w: f64, h: f64, color: [f64; 3]) {
        let [r, g, b] = color;
        self.bytes.extend_from_slice(format!("q {r} {g} {b} rg {x} {y} {w} {h} re f Q\n").as_bytes());
    }
    fn line(&mut self, x: f64, y: f64, w: f64) {
        self.rect(x, y, w, 0.6, [0.80, 0.85, 0.87]);
    }
    fn text(&mut self, text: &str, x: f64, y: f64, size: f64, font: &str, color: [f64; 3]) {
        let [r, g, b] = color;
        self.bytes.extend_from_slice(format!("q {r} {g} {b} rg BT /{font} {size} Tf 1 0 0 1 {x} {y} Tm ").as_bytes());
        self.bytes.extend(literal(&win_ansi(text)));
        self.bytes.extend_from_slice(b" Tj ET Q\n");
    }
    fn paragraph(&mut self, text: &str, x: f64, y: f64, width: f64, size: f64, max_lines: usize) -> f64 {
        let mut y = y;
        for line in wrap(text, size, width).into_iter().take(max_lines) {
            self.text(&line, x, y, size, "F1", INK);
            y -= size * 1.55;
        }
        y
    }
    fn header(&mut self, title: &str, subtitle: &str) {
        self.rect(0.0, self.h - 12.0, self.w, 12.0, ACCENT);
        self.text("YOUR ORGANIZATION", 48.0, self.h - 48.0, 9.0, "F2", ACCENT);
        self.text(title, 48.0, self.h - 94.0, 28.0, "F2", INK);
        self.paragraph(subtitle, 48.0, self.h - 119.0, self.w - 96.0, 10.0, 2);
    }
    fn section(&mut self, title: &str, y: f64) {
        self.text(title, 48.0, y, 12.0, "F2", ACCENT);
        self.paragraph(BODY, 48.0, y - 25.0, self.w - 96.0, 10.5, 5);
    }
}

pub fn create(id: &str, paper: &str) -> Result<Document, CreateError> {
    let template = find(id)?;
    let (w, h) = dimensions(id, paper)?;
    if id == "blank" {
        return super::blank(w, h, 1);
    }
    let mut doc = Document::new_empty();
    let mut fonts = Dict::new();
    for (name, face) in [("F1", "Helvetica"), ("F2", "Helvetica-Bold"), ("F3", "Times-Roman")] {
        let mut font = Dict::new();
        font.set(b"Type".to_vec(), Object::name("Font"));
        font.set(b"Subtype".to_vec(), Object::name("Type1"));
        font.set(b"BaseFont".to_vec(), Object::name(face));
        font.set(b"Encoding".to_vec(), Object::name("WinAnsiEncoding"));
        fonts.set(name.as_bytes().to_vec(), Object::Ref(doc.add(font)));
    }
    let mut resources = Dict::new();
    resources.set(b"Font".to_vec(), Object::Dict(fonts));
    for page in 0..template.pages {
        let mut c = Canvas { bytes: Vec::new(), w, h };
        draw(&mut c, template, page, paper)?;
        if id != "business-card" {
            c.line(48.0, 40.0, w - 96.0);
            c.text(template.title, 48.0, 25.0, 8.0, "F1", MUTED);
            c.text(&format!("{} / {}", page + 1, template.pages), w - 80.0, 25.0, 8.0, "F1", MUTED);
        }
        add_page(&mut doc, w, h, resources.clone(), Some(c.bytes))?;
    }
    set_title(&mut doc, template.title);
    Ok(doc)
}

fn draw(c: &mut Canvas, t: &Starter, page: usize, paper: &str) -> Result<(), CreateError> {
    let (w, h) = (c.w, c.h);
    match t.id {
        "business-card" => {
            c.rect(0.0, 0.0, 8.0, h, ACCENT);
            c.text("YOUR NAME", 24.0, h - 40.0, 18.0, "F2", INK);
            c.text("Your role / Your studio", 24.0, h - 60.0, 9.0, "F1", MUTED);
            c.line(24.0, h - 74.0, w - 48.0);
            c.text("hello@your-studio.example", 24.0, h - 96.0, 8.5, "F1", INK);
            c.text("+1 555 0100 / your-studio.example", 24.0, h - 113.0, 8.5, "F1", INK);
        }
        "flyer" => {
            c.rect(0.0, h * 0.48, w, h * 0.52, INK);
            c.rect(w * 0.62, h * 0.58, w * 0.38, h * 0.30, ACCENT);
            c.text("SOMETHING", 48.0, h - 120.0, 38.0, "F2", [1.0; 3]);
            c.text("WORTH SHARING", 48.0, h - 168.0, 34.0, "F2", [1.0; 3]);
            c.text("YOUR EVENT / YOUR BIG IDEA", 48.0, h - 210.0, 11.0, "F1", [0.72, 0.88, 0.90]);
            c.text("Make a memorable first impression.", 48.0, h * 0.42, 20.0, "F2", INK);
            c.paragraph(BODY, 48.0, h * 0.37, w - 96.0, 12.0, 5);
            c.rect(48.0, 90.0, w - 96.0, 68.0, PALE);
            c.text("DATE / TIME / PLACE", 64.0, 130.0, 12.0, "F2", ACCENT);
            c.text("Learn more at your-website.example", 64.0, 108.0, 11.0, "F1", INK);
        }
        "poster" => {
            c.rect(0.0, 55.0, w, h - 55.0, INK);
            c.rect(w - 155.0, h - 175.0, 155.0, 105.0, ACCENT);
            c.text("MEET / LEARN / CONNECT", 48.0, h - 65.0, 10.0, "F2", [0.65, 0.85, 0.87]);
            c.text("YOUR NEXT", 48.0, h - 160.0, 37.0, "F2", [1.0; 3]);
            c.text("BIG EVENT", 48.0, h - 225.0, 54.0, "F2", [1.0; 3]);
            c.paragraph(
                "A memorable experience starts with a clear invitation. Add your event's purpose and the people it brings together.",
                48.0,
                h - 280.0,
                w - 120.0,
                13.0,
                5,
            );
            c.rect(48.0, 125.0, w - 96.0, 110.0, ACCENT);
            c.text("DATE / TIME / VENUE", 65.0, 193.0, 19.0, "F2", [1.0; 3]);
            c.text("Reserve your place / your-website.example", 65.0, 160.0, 12.0, "F1", [1.0; 3]);
        }
        "brochure" => {
            for col in 0..3 {
                let x = col as f64 * w / 3.0;
                if col == 2 && page == 0 {
                    c.rect(x, 0.0, w / 3.0, h, PALE);
                }
                c.rect(x + 24.0, h - 48.0, 40.0, 5.0, ACCENT);
                let title =
                    if page == 0 { ["Get in touch", "Why choose us", "Your next chapter"] } else { ["Our story", "What we offer", "Make it happen"] };
                c.paragraph(title.get(col).copied().unwrap_or("Overview"), x + 24.0, h - 95.0, w / 3.0 - 48.0, 21.0, 3);
                c.paragraph(BODY, x + 24.0, h - 185.0, w / 3.0 - 48.0, 10.5, 14);
                c.text("YOUR ORGANIZATION", x + 24.0, 90.0, 9.0, "F2", ACCENT);
                if col > 0 {
                    c.rect(x, 70.0, 0.5, h - 140.0, [0.80, 0.85, 0.87]);
                }
            }
        }
        "weekly-planner" | "monthly-planner" => {
            c.header(t.title, "Undated / Make room for what matters");
            let cw = (w - 96.0) / 7.0;
            let rows = if t.id == "monthly-planner" { 6 } else { 1 };
            let height = (h - 230.0) / rows as f64;
            for (i, day) in ["MON", "TUE", "WED", "THU", "FRI", "SAT", "SUN"].iter().enumerate() {
                let x = 48.0 + i as f64 * cw;
                c.rect(x, h - 175.0, cw - 2.0, 24.0, PALE);
                c.text(day, x + 8.0, h - 167.0, 9.0, "F2", ACCENT);
                for row in 0..rows {
                    let top = h - 182.0 - row as f64 * height;
                    c.rect(x, top - height + 3.0, 0.5, height, [0.80, 0.85, 0.87]);
                    c.line(x, top - height + 3.0, cw - 2.0);
                }
            }
        }
        "resume" => {
            c.rect(0.0, 55.0, 185.0, h - 55.0, INK);
            c.text("YOUR", 215.0, h - 85.0, 34.0, "F2", INK);
            c.text("NAME", 215.0, h - 126.0, 34.0, "F2", ACCENT);
            c.text("PROFESSIONAL TITLE", 215.0, h - 158.0, 10.0, "F2", MUTED);
            for (i, title) in ["CONTACT", "SKILLS", "EDUCATION"].iter().enumerate() {
                let y = h - 100.0 - i as f64 * 190.0;
                c.text(title, 28.0, y, 11.0, "F2", [0.60, 0.83, 0.85]);
                for j in 0..4 {
                    c.text("Your details here", 28.0, y - 32.0 - j as f64 * 23.0, 10.0, "F1", [1.0; 3]);
                }
            }
            for (i, title) in ["PROFILE", "EXPERIENCE", "SELECTED ACHIEVEMENTS"].iter().enumerate() {
                let y = h - 210.0 - i as f64 * 172.0;
                c.text(title, 215.0, y, 11.0, "F2", ACCENT);
                c.paragraph(BODY, 215.0, y - 28.0, w - 255.0, 10.0, 7);
            }
        }
        "letter" | "cover-letter" | "press-release" => {
            c.header(if t.id == "press-release" { "FOR IMMEDIATE RELEASE" } else { "Your name / organization" }, "Address / Email / Phone / Website");
            c.text("Date: __________________", 48.0, h - 178.0, 11.0, "F1", MUTED);
            c.text(
                if t.id == "cover-letter" {
                    "Dear hiring team,"
                } else if t.id == "press-release" {
                    "CITY / DATE - Your headline goes here"
                } else {
                    "Dear recipient,"
                },
                48.0,
                h - 230.0,
                13.0,
                "F2",
                INK,
            );
            for i in 0..3 {
                c.paragraph(BODY, 48.0, h - 272.0 - i as f64 * 105.0, w - 96.0, 11.0, 5);
            }
            c.text(
                if t.id == "press-release" { "Media contact: Name / Email / Phone" } else { "Kind regards, / Your name" },
                48.0,
                110.0,
                11.0,
                "F2",
                ACCENT,
            );
        }
        "invoice" | "quotation" | "budget" => {
            c.header(
                t.title,
                if t.id == "budget" {
                    "Period: __________ / Currency: __________"
                } else {
                    "Reference: __________ / Date: __________ / Due: __________"
                },
            );
            c.text(if t.id == "budget" { "YOUR FINANCIAL OVERVIEW" } else { "BILL TO / CLIENT DETAILS" }, 48.0, h - 175.0, 10.0, "F2", ACCENT);
            c.rect(48.0, h - 245.0, w - 96.0, 30.0, PALE);
            c.text(if t.id == "budget" { "CATEGORY" } else { "DESCRIPTION" }, 60.0, h - 234.0, 10.0, "F2", INK);
            c.text(if t.id == "budget" { "PLANNED / ACTUAL" } else { "QTY / RATE / AMOUNT" }, w - 215.0, h - 234.0, 10.0, "F2", INK);
            for i in 0..7 {
                let y = h - 278.0 - i as f64 * 40.0;
                c.text(if t.id == "budget" { "Income / expense / savings" } else { "Item or service description" }, 60.0, y, 10.0, "F1", INK);
                if t.id == "budget" {
                    c.text("0.00", w - 180.0, y, 11.0, "F1", INK);
                } else {
                    c.text("1", w - 215.0, y, 11.0, "F1", INK);
                    c.text("0.00", w - 165.0, y, 11.0, "F1", INK);
                }
                c.text("0.00", w - 100.0, y, 11.0, "F1", INK);
                c.line(48.0, y - 13.0, w - 96.0);
            }
            c.rect(w - 245.0, 128.0, 197.0, 55.0, PALE);
            c.text("TOTAL", w - 230.0, 151.0, 12.0, "F2", ACCENT);
            c.text("0.00", w - 110.0, 151.0, 16.0, "F2", INK);
            c.text(
                if t.id == "quotation" {
                    "Valid until: __________ / Accepted by: __________ / Date: __________"
                } else {
                    "Notes / payment terms / next review"
                },
                48.0,
                90.0,
                10.0,
                "F1",
                MUTED,
            );
        }
        "intake-form" | "feedback" | "order-form" | "attendance" | "checklist" => {
            c.header(t.title, if t.id == "checklist" { "Project / Date / Owner" } else { "Complete the fields below. Save a copy when finished." });
            for field in fields(t.id, paper)? {
                let [x, _, _, y] = field.rect;
                if field.checkbox {
                    c.text("Task details / next step", x + 28.0, y - 12.0, 11.0, "F1", INK);
                    c.line(x + 28.0, y - 21.0, w - x - 76.0);
                } else if t.id == "attendance" {
                    c.text(field.name.rsplit(' ').next().unwrap_or(""), 48.0, y - 15.0, 10.0, "F1", MUTED);
                } else {
                    c.text(&field.name, x, y + 10.0, 11.0, "F2", ACCENT);
                }
            }
        }
        "manuscript" => {
            c.text("AUTHOR / WORKING TITLE", 72.0, h - 48.0, 9.0, "F1", MUTED);
            c.text(&format!("Chapter {}", page + 1), 72.0, h - 112.0, 29.0, "F3", INK);
            let lines = (0..4).flat_map(|_| wrap(BODY, 11.0, w - 144.0).into_iter().chain(std::iter::once(String::new())));
            for (i, line) in lines.take(24).enumerate() {
                c.text(&line, 72.0, h - 170.0 - i as f64 * 22.0, 11.0, "F1", INK);
            }
        }
        "article" | "newsletter" | "essay" => {
            c.rect(48.0, h - 53.0, 32.0, 4.0, ACCENT);
            let title = match t.id {
                "newsletter" => {
                    if page == 0 {
                        "Your newsletter"
                    } else {
                        "News and community"
                    }
                }
                "essay" => {
                    if page == 0 {
                        "Your essay title"
                    } else if page == 1 {
                        "Develop your argument"
                    } else {
                        "Conclusions and references"
                    }
                }
                _ => {
                    if page == 0 {
                        "A story worth telling"
                    } else {
                        "Continue the conversation"
                    }
                }
            };
            c.text(title, 48.0, h - 100.0, 30.0, if t.id == "newsletter" { "F2" } else { "F3" }, INK);
            c.text(
                if t.id == "newsletter" { "ISSUE / MONTH / YEAR" } else { "BY YOUR NAME / PUBLICATION / DATE" },
                48.0,
                h - 126.0,
                9.0,
                "F2",
                MUTED,
            );
            let cols = if matches!(t.id, "article" | "newsletter") { 2 } else { 1 };
            let width = (w - 96.0 - (cols - 1) as f64 * 28.0) / cols as f64;
            for col in 0..cols {
                let x = 48.0 + col as f64 * (width + 28.0);
                for section in 0..3 {
                    let y = h - 180.0 - section as f64 * 175.0;
                    let headings = match t.id {
                        "newsletter" => ["Lead story", "Community update", "Dates for your diary"],
                        "essay" => {
                            if page == 2 {
                                ["The conclusion", "Further discussion", "References"]
                            } else {
                                ["Introduction", "Argument and evidence", "Develop the idea"]
                            }
                        }
                        _ => ["A clear beginning", "Develop the idea", "What comes next"],
                    };
                    c.text(headings.get(section).copied().unwrap_or("Your section"), x, y, 13.0, "F2", ACCENT);
                    c.paragraph(BODY, x, y - 28.0, width, 11.0, 8);
                }
            }
        }
        "booklet" => {
            if page == 0 || page == 7 {
                c.rect(0.0, h * 0.34, w, h * 0.66, INK);
                c.rect(36.0, h * 0.43, w - 72.0, 8.0, ACCENT);
                c.text(if page == 0 { "Your booklet" } else { "Keep exploring" }, 36.0, h - 110.0, 30.0, "F3", [1.0; 3]);
                c.text("A SMALL FORMAT / A BIG IDEA", 36.0, h - 148.0, 9.0, "F2", [0.65, 0.85, 0.87]);
                c.paragraph("A short subtitle or closing message. Replace this text with your own story.", 36.0, h * 0.25, w - 72.0, 11.0, 5);
            } else if page == 1 {
                c.text("Contents", 36.0, h - 78.0, 26.0, "F3", INK);
                for chapter in 1..=5 {
                    let y = h - 145.0 - (chapter - 1) as f64 * 65.0;
                    c.text(&format!("Chapter {chapter} / Your title"), 36.0, y, 12.0, "F2", ACCENT);
                    c.text(&format!("{}", chapter + 2), w - 48.0, y, 12.0, "F1", INK);
                    c.line(36.0, y - 15.0, w - 72.0);
                }
            } else {
                c.text(&format!("Chapter {}", page - 1), 36.0, h - 78.0, 26.0, "F3", INK);
                for section in 0..3 {
                    c.paragraph(BODY, 36.0, h - 130.0 - section as f64 * 140.0, w - 72.0, 10.5, 7);
                }
            }
        }
        "timeline" => {
            c.header(t.title, "Project / Owner / Start date / Target date");
            let cw = (w - 268.0) / 4.0;
            for column in 0..4 {
                let x = 220.0 + column as f64 * cw;
                c.text(&format!("PERIOD {}", column + 1), x + 8.0, h - 172.0, 10.0, "F2", ACCENT);
                c.rect(x, 100.0, 0.5, h - 285.0, [0.80, 0.85, 0.87]);
            }
            for phase in 0..4 {
                let y = h - 220.0 - phase as f64 * 76.0;
                c.text(&format!("PHASE {}", phase + 1), 48.0, y, 12.0, "F2", INK);
                c.text("Owner / Due date", 48.0, y - 21.0, 10.0, "F1", MUTED);
                let x = 220.0 + phase as f64 * cw * 0.75;
                c.rect(x, y - 17.0, cw * 1.5, 28.0, PALE);
                c.text("Milestone / deliverables", x + 8.0, y - 6.0, 10.0, "F1", ACCENT);
                c.line(48.0, y - 40.0, w - 96.0);
            }
        }
        "worksheet" | "daily-planner" => {
            c.header(t.title, "Name / Date / Topic or priorities");
            for i in 0..5 {
                let y = h - 180.0 - i as f64 * 108.0;
                c.text(&format!("{} {}", if t.id == "worksheet" { "Question" } else { "Time / task" }, i + 1), 48.0, y, 12.0, "F2", ACCENT);
                for line in 0..3 {
                    c.line(48.0, y - 25.0 - line as f64 * 22.0, w - 96.0);
                }
            }
        }
        "proposal" | "report" if page == 0 => {
            c.rect(0.0, h * 0.38, w, h * 0.62, INK);
            c.rect(w - 100.0, h * 0.48, 100.0, h * 0.24, ACCENT);
            c.text(if t.id == "proposal" { "PROJECT PROPOSAL" } else { "BUSINESS REPORT" }, 48.0, h - 65.0, 10.0, "F2", [0.65, 0.85, 0.87]);
            c.text(if t.id == "proposal" { "Your project" } else { "Your report" }, 48.0, h - 155.0, 43.0, "F2", [1.0; 3]);
            c.text("A clear direction. A practical next step.", 48.0, h - 199.0, 13.0, "F1", [1.0; 3]);
            c.text("Prepared for / Prepared by / Date", 48.0, h - 242.0, 11.0, "F1", [0.72, 0.85, 0.87]);
            c.text("Executive summary", 48.0, h * 0.27, 18.0, "F2", ACCENT);
            c.paragraph(BODY, 48.0, h * 0.22, w - 96.0, 11.0, 5);
        }
        _ => {
            c.header(t.title, if page == 0 { "Your organization / Project / Date" } else { "Supporting details / Your next steps" });
            let titles: &[&str] = match t.id {
                "report" => &["Findings and supporting evidence", "Recommendations", "Next steps and ownership"],
                "proposal" => &["Scope and approach", "Deliverables and investment", "Approval and next steps"],
                "agenda" => &["01 / Welcome and desired outcomes", "02 / Discussion and decisions", "03 / Actions and next meeting"],
                "minutes" => &["Attendees and purpose", "Decisions and key discussion", "Action / Owner / Due date"],
                "project-brief" => &["The goal and intended audience", "Deliverables and constraints", "Success measures and next steps"],
                "lesson-plan" => &["Learning objectives and materials", "Activities and timing", "Assessment and reflection"],
                "study-guide" => &["Key concepts", "Review questions", "Practice and revision notes"],
                "product-sheet" => &["Product overview", "Features and specifications", "Contact and next steps"],
                _ if page == 0 => &["Executive summary", "The opportunity", "Recommended direction"],
                _ => &["Scope and approach", "Resources and milestones", "Recommendations and next steps"],
            };
            for (i, title) in titles.iter().enumerate() {
                c.section(title, h - 185.0 - i as f64 * 180.0);
            }
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn every_starter_is_a_unique_bounded_editable_pdf_layout() {
        let mut ids = std::collections::HashSet::new();
        assert!(STARTERS.len() >= 30);
        for template in STARTERS {
            assert!(ids.insert(template.id));
            for paper in ["letter", "a4"] {
                let doc = create(template.id, paper).unwrap();
                let root = doc.root().unwrap();
                let tree = doc.get(root).as_dict().unwrap().reference(b"Pages").unwrap();
                assert_eq!(doc.get(tree).as_dict().unwrap().int(b"Count"), Some(template.pages as i64));
                let (w, h) = dimensions(template.id, paper).unwrap();
                for field in fields(template.id, paper).unwrap() {
                    assert!(field.rect[0] >= 0.0 && field.rect[1] >= 0.0 && field.rect[2] <= w && field.rect[3] <= h);
                }
            }
        }
        assert_eq!(dimensions("booklet", "a4").unwrap(), (419.53, 595.28));
        assert_eq!(dimensions("booklet", "letter").unwrap(), (396.0, 612.0));
        assert!(create("missing", "letter").is_err());
        assert!(create("article", "unknown").is_err());
    }
}
