//! Moving about: the search line, the columns, the cursor and what is
//! selected.

use crate::gui::*;

impl GuiApp {
    /// The selection is the cursor alone again.
    pub fn reset_track_selection(&mut self) {
        self.record_picked = false;
        self.track_selection.clear();
        if !self.tracks.is_empty() {
            self.track_selection.insert(self.track_index);
        }
        self.track_anchor = self.track_index;
    }

    /// Escape: nothing marked any more, anywhere — the run of tracks, the
    /// rows picked in the queue, the cursor's own row. The next step, click
    /// or column marks the cursor's row again.
    pub fn let_go(&mut self) {
        self.reset_track_selection();
        self.queue_picked.clear();
        self.unmarked = true;
    }

    /// Every track in the list, from the first: cmd-a in the tracks or the
    /// albums, whose tracks they are. The cursor stays where it was, inside the
    /// run.
    pub fn select_all_tracks(&mut self) {
        if self.tracks.is_empty() {
            return;
        }
        self.unmarked = false;
        self.record_picked = false;
        self.track_selection = (0..self.tracks.len()).collect();
        self.track_anchor = 0;
        self.focus = Column::Tracks;
    }

    /// Select everything between the anchor and this row — a shift-click, or
    /// shift with an arrow key.
    pub fn extend_track_selection(&mut self, to: usize) {
        if self.tracks.is_empty() {
            return;
        }
        self.unmarked = false;
        self.record_picked = false;
        let to = to.min(self.tracks.len() - 1);
        let anchor = self.track_anchor.min(self.tracks.len() - 1);
        self.track_selection = (anchor.min(to)..=anchor.max(to)).collect();
        self.track_index = to;
        self.focus = Column::Tracks;
    }

    /// Add or remove one row without disturbing the rest — a cmd-click.
    ///
    /// The last row cannot be removed: an empty selection would leave the
    /// editor and everything else with nothing to act on, and the cursor has
    /// to be somewhere regardless.
    pub fn toggle_track_selection(&mut self, index: usize) {
        if index >= self.tracks.len() {
            return;
        }
        self.record_picked = false;
        self.unmarked = false;
        if self.track_selection.contains(&index) {
            if self.track_selection.len() > 1 {
                self.track_selection.remove(&index);
                self.track_index = *self.track_selection.iter().next().unwrap();
            }
        } else {
            self.track_selection.insert(index);
            self.track_index = index;
        }
        self.track_anchor = index;
        self.focus = Column::Tracks;
    }

    /// The tracks the selection stands for, in list order.
    pub fn selected_tracks(&self) -> Vec<&Track> {
        self.track_selection
            .iter()
            .filter_map(|i| self.tracks.get(*i))
            .collect()
    }

    /// Open the filter line, if it is not open already.
    pub fn open_filter(&mut self) {
        if self.filter.is_none() {
            self.filter = Some(String::new());
            self.focus = Column::Tracks;
        }
        self.filter_typing = true;
    }

    /// Leave the field, keeping what was found.
    ///
    /// Not [`Self::close_filter`]: the words stay in the field and the columns
    /// stay narrowed. All that ends is the keyboard being in the field — which
    /// is the whole point, because the next thing somebody does after
    /// searching is use the arrow keys on what they found.
    ///
    /// An empty field is closed outright: an open field with nothing in it is
    /// a caret and a hint, and neither is worth keeping once you have clicked
    /// somewhere else.
    pub fn stop_typing(&mut self) -> Result<()> {
        self.filter_typing = false;
        self.filter_marked = false;
        if self.filter.as_deref().is_some_and(str::is_empty) {
            return self.close_filter();
        }
        Ok(())
    }

    /// Close it and forget the query, restoring the whole library.
    pub fn close_filter(&mut self) -> Result<()> {
        self.filter_typing = false;
        self.filter_marked = false;
        self.filter_caret = None;
        if self.filter.take().is_some() {
            self.after_filter_changed()?;
        }
        Ok(())
    }

    /// Reload everything a changed filter touches.
    ///
    /// The selections are reset because they almost certainly point at rows
    /// that the new query has just removed — and because with all three columns
    /// back on "alle", their lengths describe the filter exactly, which is what
    /// the count on the filter line reports.
    pub(in crate::gui) fn after_filter_changed(&mut self) -> Result<()> {
        if self.showing_radio() {
            self.radio_typed();
            return Ok(());
        }
        if self.showing_podcasts() {
            self.podcasts_typed();
            return Ok(());
        }
        self.artist_index = if self.query().is_some() {
            ArtistRow::All.index()
        } else {
            self.home_row()
        };
        self.album_index = 0;
        self.track_index = 0;
        self.scroll_to = Some(self.focus);
        self.reload_all()
    }

    /// Whether the albums are headings in the track list rather than a
    /// column.
    pub fn merged(&self) -> bool {
        self.config.ui.albums_in_tracks
    }

    /// What stands left of the track list, for the layout.
    pub fn names_layout(&self) -> theme::Names {
        if self.merged() {
            theme::Names::One
        } else {
            theme::Names::Two
        }
    }

    /// Put the albums into the track list, or give them their column back.
    ///
    /// Into the list, the track list shows every record of the chosen artist
    /// at once, so the album choice goes back to „alle"; and a keyboard that
    /// was in the album column goes on to the tracks.
    pub fn set_albums_in_tracks(&mut self, on: bool) -> Result<()> {
        if self.config.ui.albums_in_tracks == on {
            return Ok(());
        }
        self.config.ui.albums_in_tracks = on;
        self.save_config();
        if on {
            self.album_index = 0;
            self.track_index = 0;
            if self.focus == Column::Albums {
                self.focus = Column::Tracks;
            }
        }
        self.reload_all()
    }

    /// `None` means the "all" row at the top of the column.
    pub fn selected_artist(&self) -> Option<String> {
        match self.artist_row(self.artist_index) {
            ArtistRow::Named(at) => self.artists.get(at).map(|a| a.name.clone()),
            _ => None,
        }
    }

    /// What the row at this index of the first column stands for.
    pub fn artist_row(&self, index: usize) -> ArtistRow {
        ArtistRow::at(index)
    }

    /// Where the first column comes back to when a choice has to start over:
    /// „zuletzt", when that is where the window opens, and „alle" otherwise —
    /// and always on the device, which has no „zuletzt".
    pub(in crate::gui) fn home_row(&self) -> usize {
        if self.config.ui.start_on_recent && self.config.ui.show_recent && !self.showing_ipod() {
            ArtistRow::Recent.index()
        } else {
            ArtistRow::All.index()
        }
    }

    /// Whether the "zuletzt" row is the one selected.
    pub fn showing_compilations(&self) -> bool {
        self.artist_row(self.artist_index) == ArtistRow::Compilations
    }

    pub fn showing_recent(&self) -> bool {
        self.artist_row(self.artist_index) == ArtistRow::Recent
    }

    pub fn selected_album(&self) -> Option<String> {
        if self.album_index == 0 {
            None
        } else {
            self.albums.get(self.album_index - 1).map(|a| a.name.clone())
        }
    }

    /// Whose record the selected album is.
    ///
    /// An album is a title *and* an artist — the database groups them that way
    /// — but a selection used to carry only the title, and a title alone is not
    /// unique. Two different records called "Greatest Hits" are two rows in the
    /// column and were one query underneath, so clicking either showed both.
    ///
    /// The untitled are the loud case: every track with no album tag has the
    /// same empty title, so one click on a coverless tile in the grid answered
    /// with every untagged file in the library. Carrying the artist along
    /// settles both at once.
    pub fn selected_album_artist(&self) -> Option<String> {
        if self.album_index == 0 {
            None
        } else {
            self.albums
                .get(self.album_index - 1)
                .map(|a| a.artist.clone())
        }
    }

    /// Row count of a column, counting the "all" row where there is one.
    /// Move the keyboard to another column.
    ///
    /// The bug this exists for: with a column's "alle" row hidden, index zero
    /// still means "nothing chosen here" — but there is no row at zero to draw
    /// a marker on. Arriving there by pressing right looked exactly like the
    /// key having done nothing at all. Landing on a column means landing on a
    /// row of it.
    pub fn focus_column(&mut self, column: Column) -> Result<()> {
        self.unmarked = false;
        if self.showing_radio() && column != Column::Artists {
            return Ok(());
        }
        let column = if column == Column::Albums && self.merged() {
            match self.focus {
                Column::Tracks | Column::Queue | Column::Lists => Column::Artists,
                _ => Column::Tracks,
            }
        } else {
            column
        };
        let column = if column == Column::Queue && self.lists_open() {
            Column::Lists
        } else {
            column
        };
        if column == Column::Queue && !self.queue_open() && !(self.narrow && self.fourth_open()) {
            return Ok(());
        }
        if column == Column::Lists && !self.lists_open() {
            return Ok(());
        }
        let first = self.first_index(column);
        if self.index(column) < first && self.column_len(column) > first {
            self.select(column, first)?;
        }
        self.focus = column;
        self.scroll_to = Some(column);
        Ok(())
    }

    /// The first row the keyboard may land on.
    ///
    /// Zero everywhere, except in the artist column with the "alle" row
    /// hidden: index zero still *means* "every artist" — it is where the
    /// window starts — but there is no row there to walk onto, and a selection
    /// that vanishes when you press up reads as a bug.
    pub fn first_index(&self, column: Column) -> usize {
        let mut at = 0;
        while at < self.column_len(column) && !self.drawn(column, at) {
            at += 1;
        }
        at
    }

    /// Whether there is a line at this index to land on.
    ///
    /// A row of the first column can keep its place without keeping a line:
    /// "alle" when it has been switched off, and "zuletzt" while the device is
    /// the shelf — an iPod stamps every track with the moment it was copied, so
    /// "what arrived last" is a question it cannot answer. Selecting a row
    /// nobody drew looks exactly like the keyboard having done nothing, so the
    /// keyboard steps over them.
    pub fn drawn(&self, column: Column, index: usize) -> bool {
        if column != Column::Artists {
            return true;
        }
        match self.artist_row(index) {
            ArtistRow::All => self.config.ui.show_all_artists,
            ArtistRow::Recent => self.config.ui.show_recent && !self.showing_ipod(),
            ArtistRow::Compilations => self.config.ui.show_compilations && !self.showing_ipod(),
            ArtistRow::Named(_) => true,
        }
    }

    pub fn column_len(&self, column: Column) -> usize {
        match column {
            Column::Lists => self.lists.len() + 1,
            Column::Artists => artist_rows(self.artists.len()),
            Column::Albums => self.albums.len() + 1,
            Column::Tracks => self.tracks.len(),
            Column::Queue => match self.editing {
                Some(_) => self.editing_tracks.len(),
                None => self.player.queued().len(),
            },
        }
    }

    pub fn index_mut(&mut self, column: Column) -> &mut usize {
        match column {
            Column::Lists => &mut self.lists_index,
            Column::Artists => &mut self.artist_index,
            Column::Albums => &mut self.album_index,
            Column::Tracks => &mut self.track_index,
            Column::Queue => &mut self.queue_index,
        }
    }

    pub fn index(&self, column: Column) -> usize {
        match column {
            Column::Lists => self.lists_index.min(self.lists.len()),
            Column::Artists => self.artist_index,
            Column::Albums => self.album_index,
            Column::Tracks => self.track_index,
            Column::Queue => self.queue_index,
        }
    }

    /// Selecting further left invalidates everything to the right.
    pub fn select(&mut self, column: Column, index: usize) -> Result<()> {
        self.unmarked = false;
        let len = self.column_len(column);
        if len == 0 {
            return Ok(());
        }
        *self.index_mut(column) = index.min(len - 1);

        if column == Column::Lists {
            self.focus = column;
            let at = self.lists_index;
            if at < self.lists.len() {
                self.edit_list(at)?;
            }
            return Ok(());
        }

        if column == Column::Queue {
            self.focus = column;
            return Ok(());
        }

        match column {
            Column::Artists => {
                self.album_index = 0;
                self.track_index = 0;
            }
            Column::Albums => self.track_index = 0,
            Column::Lists | Column::Tracks | Column::Queue => {}
        }
        self.focus = column;
        self.reload_dependent()
    }

    pub fn move_selection(&mut self, delta: i32) -> Result<()> {
        self.unmarked = false;
        let column = self.focus;
        if column == Column::Artists && self.showing_radio() {
            self.move_radio_cursor(delta);
            return Ok(());
        }
        if column == Column::Lists
            && delta > 0
            && self.lists_index + 1 >= self.column_len(Column::Lists)
            && !self.editing_tracks.is_empty()
        {
            self.focus = Column::Queue;
            self.queue_index = 0;
            self.scroll_to = Some(Column::Queue);
            return Ok(());
        }
        if column == Column::Queue && delta < 0 && self.queue_index == 0 && self.lists_open() {
            self.focus = Column::Lists;
            if let Some(at) = self.editing.and_then(|id| self.list_at(id)) {
                self.lists_index = at;
            }
            self.scroll_to = Some(Column::Lists);
            return Ok(());
        }
        if column == Column::Tracks && self.record_picked && delta != 0 {
            if let Some(&first) = self.track_selection.first() {
                let at = if delta > 0 { first } else { first.saturating_sub(1) };
                self.select(column, at)?;
                self.scroll_to = Some(column);
                return Ok(());
            }
        }
        let len = self.column_len(column);
        if len == 0 {
            return Ok(());
        }
        let current = self.index(column) as i32;
        let first = self.first_index(column) as i32;
        let mut next = (current + delta).clamp(first, len as i32 - 1);
        let step = if delta >= 0 { 1 } else { -1 };
        while !self.drawn(column, next as usize) {
            let further = next + step;
            if further < first || further > len as i32 - 1 {
                return Ok(());
            }
            next = further;
        }
        if next != current {
            self.select(column, next as usize)?;
            self.scroll_to = Some(column);
        }
        Ok(())
    }
}
