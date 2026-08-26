use ttyinv_core::{
    apply_edit, document, parse_json, parse_yaml, revision, serialize_markdown, to_json, to_yaml,
    validate, EditOperation, EditRequest, FontScale, FontWeight, FrameInset, Gap, SectionBody,
    TableAlignment,
};

const SOURCE: &str = include_str!("../../../examples/simple.md");

fn request(source: &str, operation: EditOperation) -> EditRequest {
    EditRequest {
        source: source.into(),
        base_revision: revision(source),
        sequence: 9,
        operation,
    }
}

#[test]
fn grammar_positive_and_manifest_roles() {
    let doc = document(SOURCE).expect("v2 sample must parse");
    let manifest = doc.structure_manifest();
    assert_eq!(manifest.ordinary_sections.len(), 2);
    assert_eq!(manifest.ordinary_sections[0].body, "table");
    assert_eq!(manifest.ordinary_sections[1].body, "prose");
    assert_eq!(manifest.fixed_blocks[0].name, "from");
    assert!(manifest.fixed_blocks.iter().all(|block| !block.movable));
}

#[test]
fn configuration_metadata_and_date_boundaries_reject() {
    for (needle, replacement) in [
        ("schema: ttyinv/v2", "schema: unsupported"),
        (
            "- Number: INV-2026-001\n",
            "- Number: INV-2026-001\n- Number: DUP\n",
        ),
        ("- Currency: EUR", "- Currency: eur"),
        ("- Issued: 2026-01-15", "- Issued: 2026-02-30"),
        ("density: comfortable", "density: invalid"),
    ] {
        assert!(
            document(&SOURCE.replace(needle, replacement)).is_err(),
            "{needle}"
        );
    }
    let unknown = SOURCE.replace("schema: ttyinv/v2", "schema: ttyinv/v2\nunknown: value");
    assert!(document(&unknown).is_err());
    let reversed = SOURCE.replace("- Due: 2026-01-29", "- Due: 2026-01-01");
    assert!(document(&reversed).is_err());
}

#[test]
fn parties_repeats_identifiers_and_images_parse() {
    let source = SOURCE
        .replace("## From\n", "## From\n\n![Seller](./seller.png)\n")
        .replace(
            "- Name: Northstar Studio",
            "- Name: Northstar Studio\n- Address: Paris\n- ID.TAX: DE123\n- ID.REG: 77",
        )
        .replace(
            "## Bill to\n",
            "## Bill to\n\n![Buyer](https://example.com/buyer.png)\n",
        )
        .replace(
            "- Name: Acme Research Ltd",
            "- Name: Acme Research Ltd\n- Address: Paris",
        );
    let doc = document(&source).expect("party fields parse");
    assert_eq!(doc.from.address.len(), 3);
    assert_eq!(doc.from.identifiers.len(), 3);
    assert_eq!(
        doc.from.logo.as_ref().map(|x| x.alt.as_str()),
        Some("Seller")
    );
}

#[test]
fn fixed_block_duplicate_and_identifier_rules_are_strict() {
    let duplicate_name = SOURCE.replace(
        "- Name: Northstar Studio",
        "- Name: Northstar Studio\n- Name: Alias",
    );
    assert!(document(&duplicate_name).is_err());

    let unsafe_identifier = SOURCE.replace(
        "- Name: Northstar Studio",
        "- Name: Northstar Studio\n- ID.9VAT: DE123",
    );
    assert!(document(&unsafe_identifier).is_err());

    let duplicate_payment =
        format!("{SOURCE}\n## Payment\n\n### Bank\n- Account: 1\n- Account: 2\n");
    assert!(document(&duplicate_payment).is_err());

    let duplicate_signature =
        format!("{SOURCE}\n## Signature\n\n- Name: Signer\n- Name: Alias\n- Label: Director\n");
    assert!(document(&duplicate_signature).is_err());
    let late_party_image = SOURCE.replace(
        "- Name: Northstar Studio",
        "- Name: Northstar Studio\n![Seller](./seller.png)",
    );
    assert!(document(&late_party_image).is_err());
}

#[test]
fn canonical_markdown_is_deterministic() {
    let doc = document(SOURCE).expect("parse");
    let first = serialize_markdown(&doc);

    let second = serialize_markdown(&document(&first).expect("canonical parse"));
    assert_eq!(first, second);
}

#[test]
fn adapters_are_equivalent_and_have_rejection_boundary() {
    let doc = document(SOURCE).expect("parse");
    let json = to_json(&doc).expect("json");
    let yaml = to_yaml(&doc).expect("yaml");
    let from_json = parse_json(&json).expect("json parse");
    let from_yaml = parse_yaml(&yaml).expect("yaml parse");
    assert_eq!(from_json.title, from_yaml.title);
    assert_eq!(from_json.metadata.currency, "EUR");
    assert!(parse_json("{\"config\":{\"schema\":\"ttyinv/v2\",\"bad\":true}}").is_err());
    let injected = json.replace(
        "Payment is due within fourteen days.",
        "Payment is due within fourteen days.\\n## Injected",
    );
    assert!(parse_json(&injected).is_err());
    let json_null = json.replace("\"currency\": \"EUR\"", "\"currency\": null");
    assert!(parse_json(&json_null).is_err());
    let yaml_null = yaml.replace("currency: EUR", "currency:");
    assert!(parse_yaml(&yaml_null).is_err());
}

#[test]
fn scalar_paths_and_source_edits() {
    let cases = [
        ("title", "Renamed"),
        ("metadata.number", "INV-2"),
        ("metadata.currency", "USD"),
        ("sections[0].title", "Fees"),
        ("sections[0].table.headings[0]", "Item"),
        ("sections[0].table.rows[0].cells[0]", "Design"),
        ("from.address[0]", "New Street"),
        ("from.identifiers.VAT", "US123"),
    ];
    let mut source = SOURCE.to_owned();
    for (path, value) in cases {
        let response = apply_edit(request(
            &source,
            EditOperation::SetScalar {
                path: path.into(),
                value: value.into(),
            },
        ));
        assert!(!response.conflict, "{path}");
        assert!(
            response.diagnostics.is_empty(),
            "{path}: {:?}",
            response.diagnostics
        );
        source = response.source;
    }
    assert!(source.contains("Design"));
    assert!(apply_edit(request(
        &source,
        EditOperation::SetScalar {
            path: "hostile.path".into(),
            value: "x".into()
        }
    ))
    .diagnostics
    .iter()
    .any(|d| d.code == "EDIT003"));
}

#[test]
fn move_section_preserves_directives_and_rejects_conflicts() {
    let source = SOURCE.replace("## Notes", "<!-- ttyinv:page-break-before -->\n## Notes");
    let response = apply_edit(request(
        &source,
        EditOperation::MoveSection { from: 1, to: 0 },
    ));
    assert!(!response.conflict);
    assert!(
        response.source.find("page-break-before").unwrap()
            < response.source.find("## Notes").unwrap()
    );
    let invalid = apply_edit(request(
        &source,
        EditOperation::MoveSection { from: 99, to: 0 },
    ));
    assert!(invalid.diagnostics.iter().any(|d| d.code == "EDIT002"));
    let stale = apply_edit(EditRequest {
        source,
        base_revision: "stale".into(),
        sequence: 1,
        operation: EditOperation::MoveSection { from: 0, to: 1 },
    });
    assert!(stale.conflict);
}

#[test]
fn gap_insert_update_remove_and_first_section() {
    let roomy = apply_edit(request(
        SOURCE,
        EditOperation::SetSectionGap {
            section: 0,
            gap: Gap::Roomy,
        },
    ))
    .source;
    assert!(roomy.contains("gap-before roomy"));
    let tight = apply_edit(request(
        &roomy,
        EditOperation::SetSectionGap {
            section: 0,
            gap: Gap::Tight,
        },
    ))
    .source;
    assert!(!tight.contains("gap-before roomy"));
    assert!(tight.contains("gap-before tight"));
    let standard = apply_edit(request(
        &tight,
        EditOperation::SetSectionGap {
            section: 0,
            gap: Gap::Standard,
        },
    ))
    .source;
    assert!(!standard.contains("gap-before tight"));
}

#[test]
fn normalized_input_and_strict_reserved_content() {
    let bom_crlf = format!("\u{feff}{}", SOURCE.replace('\n', "\r\n"));
    assert!(document(&bom_crlf).is_ok());
    let indented = SOURCE.replace("## From", " ## From");
    assert!(document(&indented).is_err());
    let unknown = SOURCE.replace("## From\n", "## From\n- Transfer: arbitrary\n");
    assert!(document(&unknown).is_err());
    let separated_directive =
        SOURCE.replace("## Notes", "<!-- ttyinv:page-break-before -->\n\n## Notes");
    assert!(document(&separated_directive).is_err());
}

#[test]
fn escaped_pipe_alignment_and_typed_footer_break() {
    let source = SOURCE
        .replace("|---|---:|---:|---:|", "|---|:---:|---:|---:|")
        .replace("Systems consulting", r"Systems \| consulting")
        .replace("## Notes", "<!-- ttyinv:page-break-before -->\n## Notes");
    let doc = document(&source).expect("escaped table");
    let SectionBody::Table(table) = &doc.ordinary_sections[0].body else {
        panic!("table")
    };
    assert_eq!(table.rows[0][0], "Systems | consulting");
    assert_eq!(table.alignments[1], TableAlignment::Center);
    let footer = format!(
        "{source}\n## Settlements\n\n| Date | Paid | Paid currency | Received | Received currency |\n|---|---:|---|---:|---|\n| 2026-01-20 | 1.00 | EUR | 1.00 | EUR |\n"
    );
    assert!(document(&footer).is_ok());
}

#[test]
fn move_last_ordinary_stays_before_footer_and_gap_preserves_flags() {
    let source = SOURCE
        .replace("## Notes", "<!-- ttyinv:page-break-before -->\n<!-- ttyinv:summary-only -->\n<!-- ttyinv:gap-before roomy -->\n## Notes");
    let with_footer = format!(
        "{source}\n## Payment\n\n### Bank\n- Account: 1\n\n## Signature\n\n- Name: Signer\n- Label: Director\n"
    );
    let moved = apply_edit(request(
        &with_footer,
        EditOperation::MoveSection { from: 0, to: 1 },
    ));
    assert!(moved.diagnostics.is_empty(), "{:?}", moved.diagnostics);
    assert!(
        moved.source.find("## Payment").unwrap() > moved.source.find("## Contract fees").unwrap()
    );
    let gapped = apply_edit(request(
        &with_footer,
        EditOperation::SetSectionGap {
            section: 1,
            gap: Gap::Tight,
        },
    ));
    assert!(gapped.source.contains("summary-only"));
    assert!(gapped.source.contains("page-break-before"));
}

#[test]
fn fences_and_table_delimiters_follow_markdown_rules() {
    let fenced = SOURCE.replace(
        "Payment is due within fourteen days.",
        "````rust\n## From\n<!-- ttyinv:gap-before roomy -->\n````\nPayment is due within fourteen days.",
    );
    assert!(document(&fenced).is_ok());

    let short_separator = SOURCE.replace("|---|---:|---:|---:|", "|--|--:|--:|--:|");
    assert!(document(&short_separator).is_err());

    let empty_cell = SOURCE.replace(
        "| Systems consulting | 8 | 650.00 | auto |",
        "| Systems consulting | 8 | 650.00 |  |",
    );
    assert!(document(&empty_cell).is_ok());
    let extra_h1 = SOURCE.replace("Payment is due within fourteen days.", "# Hidden title");
    assert!(document(&extra_h1).is_err());
}

#[test]
fn invalid_scalar_keeps_manifest_and_adapters_validate() {
    let response = apply_edit(request(
        SOURCE,
        EditOperation::SetScalar {
            path: "metadata.currency".into(),
            value: "eur".into(),
        },
    ));
    assert!(response.diagnostics.iter().any(|d| d.code == "CURRENCY001"));
    assert!(ttyinv_core::structure_manifest(&response.source).is_ok());
    let injected = serde_json::to_string(&document(SOURCE).unwrap())
        .unwrap()
        .replace("Northstar Studio", "Northstar\n## Injected");
    assert!(parse_json(&injected).is_err());
    let oversized = apply_edit(request(
        SOURCE,
        EditOperation::SetScalar {
            path: "title".into(),
            value: "x".repeat(ttyinv_core::MAX_EDIT_BYTES),
        },
    ));
    assert!(oversized.diagnostics.iter().any(|d| d.code == "LIMIT001"));
}

#[test]
fn hostile_malformed_inputs_return_diagnostics_without_panicking() {
    for source in [
        "",
        "\u{feff}",
        "---\n[\n---\n# \u{10ffff}",
        "---\nschema: ttyinv/v2\n---\n# Title\n## From\n![\n",
        "---\nschema: ttyinv/v2\n---\n# Title\n## From\n- Name: A\n## Bill to\n- Name: B\n## Work\n| x |\n|--|\n| |\n",
        "---\nschema: ttyinv/v2\n---\n# Title\n## From\n- Name: A\n## Bill to\n- Name: B\n## Work\n```rust\n## Fake\n| a |\n| b |\n````\n",
    ] {
        assert!(
            std::panic::catch_unwind(|| validate(source)).is_ok(),
            "input panicked: {source:?}"
        );
    }
}

#[test]
fn concurrent_atomic_writes_leave_one_complete_document() {
    use std::thread;
    let path = std::env::temp_dir().join(format!(
        "ttyinv-v2-atomic-{}-{}",
        std::process::id(),
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos()
    ));
    let sources = (0..8)
        .map(|i| SOURCE.replace("# Consulting services", &format!("# Invoice {i}")))
        .collect::<Vec<_>>();
    thread::scope(|scope| {
        for source in &sources {
            let path = path.clone();
            scope.spawn(move || {
                ttyinv_core::atomic_write(&path, source).expect("atomic write");
            });
        }
    });
    let written = std::fs::read_to_string(&path).expect("written invoice");
    assert!(sources.iter().any(|source| source == &written));
    assert!(document(&written).is_ok());
    std::fs::remove_file(path).expect("remove invoice");
}

#[test]
fn money_columns_validate_and_expose_totals() {
    use rust_decimal::Decimal;

    let exact = document(SOURCE).expect("exact amount");
    assert_eq!(
        exact.ordinary_sections[0].total,
        Some(Decimal::new(5_200, 0))
    );
    assert_eq!(exact.grand_total, Decimal::new(5_200, 0));

    let rounded = SOURCE.replace("8 | 650.00 | auto", "2.5 | 6.67 | 16.66");
    assert!(document(&rounded).is_ok());
    let outside = rounded.replace("16.66", "16.65");
    assert!(validate(&outside)
        .diagnostics()
        .iter()
        .any(|d| d.code == "MONEY004"));

    let missing = SOURCE
        .replace(
            "Description | Days | Rate | Amount (EUR)",
            "Description | Amount (EUR)",
        )
        .replace("|---|---:|---:|---:|", "|---|---:|")
        .replace(
            "Systems consulting | 8 | 650.00 | auto",
            "Systems consulting | auto",
        );
    let missing_report = validate(&missing);
    assert!(
        missing_report
            .diagnostics()
            .iter()
            .any(|d| d.code == "MONEY003"),
        "{:?}",
        missing_report.diagnostics()
    );

    let explicit_only = missing.replace("Systems consulting | auto", "Systems consulting | 10.00");
    assert!(document(&explicit_only).is_ok());

    let jpy = explicit_only
        .replace("Currency: EUR", "Currency: JPY")
        .replace("Amount (EUR)", "Amount (JPY)")
        .replace("10.00", "1.5");
    let jpy_doc = document(&jpy).expect("JPY amount");
    assert_eq!(jpy_doc.grand_total, Decimal::new(2, 0));

    let bhd = explicit_only
        .replace("Currency: EUR", "Currency: BHD")
        .replace("Amount (EUR)", "Amount (BHD)")
        .replace("10.00", "1.234");
    assert_eq!(
        document(&bhd).expect("BHD amount").grand_total,
        Decimal::new(1234, 3)
    );
}

#[test]
fn prose_scalar_edits_are_single_line_and_stop_before_directives() {
    let source = format!(
        "{}\n<!-- ttyinv:page-break-before -->\n## Payment\n\n### Bank\n- Account: 1\n",
        SOURCE.trim_end()
    );
    let edited = apply_edit(request(
        &source,
        EditOperation::SetScalar {
            path: "sections[1].prose".into(),
            value: "Updated payment terms.".into(),
        },
    ));
    assert!(edited.diagnostics.is_empty(), "{:?}", edited.diagnostics);
    assert!(edited.source.contains("Updated payment terms."));
    assert!(edited.source.contains("<!-- ttyinv:page-break-before -->"));
    assert!(edited.source.contains("## Payment"));

    let normalized = format!("\u{feff}{}", source.replace('\n', "\r\n"));
    let edited = apply_edit(request(
        &normalized,
        EditOperation::SetScalar {
            path: "sections[1].prose".into(),
            value: "CRLF remains.".into(),
        },
    ));
    assert!(edited.diagnostics.is_empty(), "{:?}", edited.diagnostics);
    assert!(edited.source.starts_with('\u{feff}'));
    assert!(edited
        .source
        .contains("\r\n<!-- ttyinv:page-break-before -->\r\n"));
    assert!(!edited.source.replace("\r\n", "").contains('\n'));
}

#[test]
fn prose_scalar_edits_reject_structure_and_non_prose_targets() {
    for value in [
        "one\n\ntwo",
        "| a | b |",
        "## New block",
        "<!-- ttyinv:summary-only -->",
    ] {
        let response = apply_edit(request(
            SOURCE,
            EditOperation::SetScalar {
                path: "sections[1].prose".into(),
                value: value.into(),
            },
        ));
        assert!(
            response.diagnostics.iter().any(|d| d.code == "EDIT003"),
            "{value:?}: {:?}",
            response.diagnostics
        );
        assert_eq!(response.source, SOURCE);
    }

    let response = apply_edit(request(
        SOURCE,
        EditOperation::SetScalar {
            path: "sections[0].prose".into(),
            value: "not a table".into(),
        },
    ));
    assert!(response.diagnostics.iter().any(|d| d.code == "EDIT003"));
    assert_eq!(response.source, SOURCE);
}

#[test]
fn metadata_scalar_range_does_not_search_footer_blocks() {
    let source = format!(
        "{}\n## Payment\n\n### Bank\n- Terms: keep\n- Account: 1\n",
        SOURCE.replace("- Terms: Net 14\n", "")
    );
    let response = apply_edit(request(
        &source,
        EditOperation::SetScalar {
            path: "metadata.terms".into(),
            value: "Net 30".into(),
        },
    ));
    assert!(response.diagnostics.iter().any(|d| d.code == "EDIT004"));
    assert_eq!(response.source, source);
}

#[test]
fn summary_detection_uses_description_alias_column() {
    let source = SOURCE
        .replace(
            "Description | Days | Rate | Amount (EUR)",
            "Category | Description | Days | Rate | Amount (EUR)",
        )
        .replace("|---|---:|---:|---:|", "|---|---|---:|---:|---:|")
        .replace(
            "| Systems consulting | 8 | 650.00 | auto |",
            "| Line item | Systems consulting | 8 | 650.00 | auto |",
        )
        .replace(
            "| Line item | Systems consulting | 8 | 650.00 | auto |\n",
            "| Line item | Systems consulting | 8 | 650.00 | auto |\n| Label | Total | 0 | 0 | auto |\n",
        );
    let report = validate(&source);
    assert!(
        report.diagnostics().iter().any(|d| d.code == "MONEY008"),
        "{:?}",
        report.diagnostics()
    );
}

#[test]
fn rate_scale_and_minor_unit_rounding_define_money_tolerance() {
    let strict = SOURCE.replace("8 | 650.00 | auto", "1000 | 1.0000 | 995");
    assert!(validate(&strict)
        .diagnostics()
        .iter()
        .any(|d| d.code == "MONEY004"));

    let boundary = SOURCE.replace("8 | 650.00 | auto", "1000 | 1.0000 | 999.945");
    assert!(document(&boundary).is_ok());

    let half_even = SOURCE.replace("8 | 650.00 | auto", "1 | 1.005 | 1.005");
    let doc = document(&half_even).expect("half-even amount rounding");
    assert_eq!(doc.grand_total, rust_decimal::Decimal::new(100, 2));
}

#[test]
fn appearance_defaults_boundaries_and_strict_values() {
    let without_appearance = SOURCE
        .lines()
        .filter(|line| {
            !line.starts_with("accent:")
                && !line.starts_with("font-weight:")
                && !line.starts_with("font-scale:")
                && !line.starts_with("frame-inset:")
        })
        .collect::<Vec<_>>()
        .join("\n");
    let defaults = document(&without_appearance).expect("appearance defaults");
    assert!(defaults.config.accent.is_none());
    assert_eq!(defaults.config.font_weight, FontWeight::Regular);
    assert_eq!(defaults.config.font_scale, FontScale::default());
    assert_eq!(defaults.config.frame_inset, FrameInset::default());

    for (field, low, high) in [("font-scale", "100", "140"), ("frame-inset", "30", "60")] {
        for value in [low, high] {
            let source = SOURCE.replace(
                &format!(
                    "{field}: {}",
                    if field == "font-scale" { "100" } else { "54" }
                ),
                &format!("{field}: {value}"),
            );
            assert!(document(&source).is_ok(), "{field}={value}");
        }
    }
    let weighted = SOURCE.replace("font-weight: regular", "font-weight: semibold");
    assert_eq!(
        document(&weighted)
            .expect("semibold weight")
            .config
            .font_weight,
        FontWeight::Semibold
    );
    for replacement in [
        ("accent: \"#2f6fed\"", "accent: \"#2F6FED\""),
        ("accent: \"#2f6fed\"", "accent: \"#fff\""),
        ("font-scale: 100", "font-scale: 99"),
        ("font-scale: 100", "font-scale: 141"),
        ("frame-inset: 54", "frame-inset: 29"),
        ("frame-inset: 54", "frame-inset: 61"),
        ("font-weight: semibold", "font-weight: bold"),
        ("font-weight: semibold", "font-weight: 600"),
        ("font-weight: semibold", "font_weight: semibold"),
        ("font-scale: 100", "font-scale: nope"),
    ] {
        assert!(document(&weighted.replace(replacement.0, replacement.1)).is_err());
    }
}

#[test]
fn appearance_canonical_order_adapters_and_edits() {
    let doc = document(SOURCE).expect("appearance source");
    let markdown = serialize_markdown(&doc);
    let font = markdown.find("font:").expect("font");
    let weight = markdown.find("font-weight:").expect("font weight");
    let density = markdown.find("density:").expect("density");
    let accent = markdown.find("accent:").expect("accent");
    let scale = markdown.find("font-scale:").expect("font scale");
    let inset = markdown.find("frame-inset:").expect("frame inset");
    assert!(
        font < weight && weight < density && density < accent && accent < scale && scale < inset
    );
    assert!(markdown.contains("font-weight: regular"));
    assert!(markdown.contains("accent: \"#2f6fed\""));

    let weighted = SOURCE.replace("font-weight: regular", "font-weight: semibold");
    let doc = document(&weighted).expect("semibold source");
    assert_eq!(doc.config.font_weight, FontWeight::Semibold);
    let json = to_json(&doc).expect("JSON");
    let yaml = to_yaml(&doc).expect("YAML");
    assert!(json.contains("\"font_weight\": \"semibold\""));
    assert!(yaml.contains("font_weight: semibold"));
    assert_eq!(
        parse_json(&json).expect("JSON parse").config,
        parse_yaml(&yaml).expect("YAML parse").config
    );
    for (needle, replacement) in [
        (
            "\"font_weight\": \"semibold\"",
            "\"font_weight\": \"regular\"",
        ),
        ("\"font_weight\": \"semibold\"", "\"font_weight\": \"bold\""),
        ("\"font_weight\": \"semibold\"", "\"font_weight\": 600"),
    ] {
        let candidate = json.replace(needle, replacement);
        if replacement.ends_with("regular\"") {
            assert_eq!(
                parse_json(&candidate)
                    .expect("regular JSON")
                    .config
                    .font_weight,
                FontWeight::Regular
            );
        } else {
            assert!(parse_json(&candidate).is_err());
        }
    }

    let edited = apply_edit(request(
        &weighted,
        EditOperation::SetScalar {
            path: "config.font_weight".into(),
            value: "regular".into(),
        },
    ));
    assert!(edited.diagnostics.is_empty(), "{:?}", edited.diagnostics);
    assert!(edited.source.contains("font-weight: regular"));
    assert!(!edited.source.contains("font-weight: semibold"));
    assert!(edited.source.contains("accent: \"#2f6fed\""));
    assert!(edited.source.contains("## Contract fees"));

    let without_weight = SOURCE.replace("font-weight: regular\n", "");
    let inserted = apply_edit(request(
        &without_weight,
        EditOperation::SetScalar {
            path: "config.font_weight".into(),
            value: "semibold".into(),
        },
    ));
    assert!(
        inserted.diagnostics.is_empty(),
        "{:?}",
        inserted.diagnostics
    );
    let inserted_font = inserted.source.find("font:").expect("font");
    let inserted_weight = inserted.source.find("font-weight:").expect("weight");
    let inserted_density = inserted.source.find("density:").expect("density");
    assert!(inserted_font < inserted_weight && inserted_weight < inserted_density);
}
