# SPDX-License-Identifier: GPL-3.0-or-later
"""Per-format compress and extract through the real dialogs.

Creatable formats round-trip: Compress… with that format selected, then
Extract to…, then the original bytes. Extract-only formats are covered by the
Rust suite, which can write those codecs without the compress dialog.
"""

from __future__ import annotations

import pytest

CREATABLE_FORMATS = (
    ("ZIP", "bundle.zip"),
    ("TAR.GZ", "bundle.tar.gz"),
    ("TAR", "bundle.tar"),
)


def compress_as(strata, entry_name, archive_name, format_label):
    strata.open_context_menu(entry_name)
    strata.choose_menu_item("Compress…")
    dialog = strata.wait_for_dialog()
    if format_label != "ZIP":
        option = strata.wait(
            lambda: dialog.find(role="toggle button", name=format_label),
            f"the {format_label} format option",
        )
        assert option.activate(), (
            f"{format_label} should expose an accessible action"
        )
    strata.wait(
        lambda: dialog.find(role="toggle button", name="No password")
        is None,
        "password options to stay hidden: exarch_core cannot write encrypted archives",
    )
    field = strata.wait(
        lambda: dialog.find(role="text", states={"editable"}),
        "the archive name field",
    )
    strata.pointer.click(field)
    field = strata.editable_field()
    strata.keyboard.press("ctrl+a")
    strata.keyboard.type_text(archive_name)
    strata.wait(
        lambda: field.text == archive_name, f"{archive_name!r} to reach the name field"
    )
    strata.pointer.click(strata.dialog_button("Compress"))
    strata.wait(lambda: strata.dialog() is None, "the compress dialog to close")


def extract_to(strata, archive_name, destination):
    strata.open_context_menu(archive_name)
    strata.choose_menu_item("Extract to…")
    field = strata.editable_field()
    strata.keyboard.press("ctrl+a")
    strata.keyboard.type_text(str(destination))
    strata.wait(
        lambda: field.text == str(destination), "the destination to reach the field"
    )
    strata.keyboard.press("Return")


@pytest.mark.parametrize(
    "format_label, archive_name",
    CREATABLE_FORMATS,
    ids=["zip", "tar-gz", "tar"],
)
def test_round_trip_from_the_context_menu(strata, format_label, archive_name):
    fixture = strata.fixture
    original = fixture.path("readme.md").read_text()

    compress_as(strata, "readme.md", "bundle", format_label)
    strata.wait(
        lambda: fixture.path(archive_name).exists(),
        f"{archive_name} to be created",
    )
    assert fixture.path("readme.md").read_text() == original, (
        "compression must leave the source in place"
    )

    destination = fixture.path("unpacked")
    extract_to(strata, archive_name, destination)
    extracted = destination / "readme.md"
    strata.wait(
        lambda: extracted.exists(),
        f"{archive_name} to extract into the destination",
    )
    assert extracted.read_text() == original, (
        f"{format_label} should restore the original contents"
    )
