use crate::gui::*;

impl GuiApp {
    pub fn reset_track_selection(&mut self) {
        self.record_picked = false;
        self.track_selection.clear();
        if !self.tracks.is_empty() {
            self.track_selection.insert(self.track_index);
        }
        self.track_anchor = self.track_index;
    }

    pub fn let_go(&mut self) {
        self.reset_track_selection();
        self.queue_picked.clear();
        self.unmarked = true;
    }

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

    pub fn selected_tracks(&self) -> Vec<&Track> {
        self.track_selection
            .iter()
            .filter_map(|i| self.tracks.get(*i))
            .collect()
    }

    pub fn open_filter(&mut self) {
        if self.filter.is_none() {
            self.filter = Some(String::new());
            self.focus = Column::Tracks;
        }
        self.filter_typing = true;
    }

    pub fn stop_typing(&mut self) -> Result<()> {
        self.filter_typing = false;
        self.filter_marked = false;
        if self.filter.as_deref().is_some_and(str::is_empty) {
            return self.close_filter();
        }
        Ok(())
    }

    pub fn close_filter(&mut self) -> Result<()> {
        self.filter_typing = false;
        self.filter_marked = false;
        self.filter_caret = None;
        if self.filter.take().is_some() {
            self.after_filter_changed()?;
        }
        Ok(())
    }

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

    pub fn merged(&self) -> bool {
        self.config.ui.albums_in_tracks
    }

    pub fn names_layout(&self) -> theme::Names {
        if self.merged() {
            theme::Names::One
        } else {
            theme::Names::Two
        }
    }

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

    pub fn selected_artist(&self) -> Option<String> {
        match self.artist_row(self.artist_index) {
            ArtistRow::Named(at) => self.artists.get(at).map(|a| a.name.clone()),
            _ => None,
        }
    }

    pub fn artist_row(&self, index: usize) -> ArtistRow {
        ArtistRow::at(index)
    }

    pub(in crate::gui) fn home_row(&self) -> usize {
        if self.config.ui.start_on_recent && self.config.ui.show_recent && !self.showing_ipod() {
            ArtistRow::Recent.index()
        } else {
            ArtistRow::All.index()
        }
    }

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

    pub fn selected_album_artist(&self) -> Option<String> {
        if self.album_index == 0 {
            None
        } else {
            self.albums
                .get(self.album_index - 1)
                .map(|a| a.artist.clone())
        }
    }

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

    pub fn first_index(&self, column: Column) -> usize {
        let mut at = 0;
        while at < self.column_len(column) && !self.drawn(column, at) {
            at += 1;
        }
        at
    }

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
