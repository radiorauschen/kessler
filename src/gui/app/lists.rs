use crate::gui::*;

impl GuiApp {
    pub fn ask_for_a_list(&mut self, tracks: Vec<Track>) {
        let suggestion = suggest_list_name(&tracks);
        self.close_panels();
        self.prompt = Some(Prompt {
            entry: tagedit::Entry::new(suggestion),
            ask: Ask::NewList(tracks),
        });
        self.open_lists();
        self.lists_index = self.lists.len();
        self.focus = self.prompt_column();
        self.scroll_to = Some(self.focus);
        if self.prompt_inline() {
            self.set_status(tr!("name der liste — enter legt sie an, esc bricht ab"));
        }
    }

    pub fn ask_to_rename_list(&mut self, at: usize) {
        let Some((list, _)) = self.lists.get(at) else {
            return;
        };
        let (id, name) = (list.id, list.name.clone());
        self.close_panels();
        self.prompt = Some(Prompt {
            entry: tagedit::Entry::new(name),
            ask: Ask::Rename(id),
        });
        self.open_lists();
        self.lists_index = at;
        self.focus = self.prompt_column();
        self.scroll_to = Some(self.focus);
        if self.prompt_inline() {
            self.set_status(tr!("neuer name — enter benennt um, esc bricht ab"));
        }
    }

    pub fn ask_to_delete_list(&mut self, at: usize) {
        let Some((list, _)) = self.lists.get(at) else {
            return;
        };
        let (id, name) = (list.id, list.name.clone());
        self.close_panels();
        self.prompt = Some(Prompt {
            entry: tagedit::Entry::new(name),
            ask: Ask::Delete(id),
        });
        self.open_lists();
        self.lists_index = at;
        self.focus = if self.lists_open() {
            Column::Lists
        } else {
            Column::Tracks
        };
        self.scroll_to = Some(self.focus);
    }

    pub(in crate::gui) fn prompt_column(&self) -> Column {
        if self.prompt_inline() {
            Column::Lists
        } else {
            Column::Tracks
        }
    }

    pub(in crate::gui) fn close_panels(&mut self) {
        self.details = None;
        self.editor = None;
        self.cover_pick = None;
        self.row_menu = None;
        self.transfer = None;
    }

    pub fn answer_prompt(&mut self) -> Result<()> {
        let Some(prompt) = self.prompt.take() else {
            return Ok(());
        };
        let name = prompt.entry.text().trim().to_string();
        if prompt.typing() && name.is_empty() {
            self.prompt = Some(prompt);
            return Ok(());
        }

        match prompt.ask {
            Ask::NewList(tracks) => {
                let id = self.db.create_playlist(&name)?;
                let ids: Vec<i64> = tracks.iter().map(|t| t.id).collect();
                if !ids.is_empty() {
                    self.db.set_playlist_tracks(id, &ids)?;
                }
                self.read_lists()?;
                if let Some(at) = self.list_at(id) {
                    self.edit_list(at)?;
                    self.lists_index = at;
                    self.focus = Column::Lists;
                    self.scroll_to = Some(Column::Lists);
                }
                self.note(
                    log::Kind::List,
                    match ids.len() {
                        0 => tr!("„{}“ angelegt", name),
                        n => tr!("„{}“ angelegt, {} titel", name, text::count(n as u64)),
                    },
                );
                self.set_status(match ids.len() {
                    0 => tr!("liste „{}“ angelegt — titel hierher ziehen", name),
                    1 => tr!("„{}“ angelegt, 1 titel", name),
                    n => tr!("„{}“ angelegt, {} titel", name, text::count(n as u64)),
                });
            }
            Ask::Rename(id) => {
                self.db.rename_playlist(id, &name)?;
                self.read_lists()?;
                if let Some(at) = self.list_at(id) {
                    self.lists_index = at;
                    self.focus = Column::Lists;
                    self.scroll_to = Some(Column::Lists);
                }
                self.note(log::Kind::List, tr!("umbenannt in „{}“", name));
                self.set_status(tr!("liste heißt jetzt „{}“", name));
            }
            Ask::Delete(id) => {
                self.db.delete_playlist(id)?;
                if self.editing == Some(id) {
                    self.stop_editing();
                }
                self.read_lists()?;
                self.lists_index = self.lists_index.min(self.lists.len());
                self.note(log::Kind::List, tr!("„{}“ gelöscht", name));
                self.set_status(tr!("liste „{}“ gelöscht", name));
            }
        }
        Ok(())
    }

    pub fn cancel_prompt(&mut self) {
        self.prompt = None;
    }

    pub(in crate::gui) fn read_lists(&mut self) -> Result<()> {
        self.lists = self.db.playlists_with_counts()?;
        Ok(())
    }

    pub(in crate::gui) fn list_at(&self, id: i64) -> Option<usize> {
        self.lists.iter().position(|(list, _)| list.id == id)
    }

    pub fn add_to_list(&mut self, at: usize, tracks: &[Track]) -> Result<()> {
        let Some((list, _)) = self.lists.get(at) else {
            return Ok(());
        };
        let (id, name) = (list.id, list.name.clone());
        let ids: Vec<i64> = tracks.iter().map(|t| t.id).collect();
        if ids.is_empty() {
            return Ok(());
        }
        self.db.append_to_playlist(id, &ids)?;
        self.note(
            log::Kind::List,
            tr!("{} titel in „{}“", text::count(ids.len() as u64), name),
        );
        self.set_status(match ids.len() {
            1 => tr!("1 titel in „{}“", name),
            n => tr!("{} titel in „{}“", text::count(n as u64), name),
        });
        if self.editing == Some(id) {
            self.editing_tracks = self.db.get_playlist_tracks(id)?;
        }
        self.read_lists()
    }

    pub fn queue_as_list(&mut self) {
        let tracks = self.player.queued().to_vec();
        if tracks.is_empty() {
            self.set_status(tr!("die warteschlange ist leer"));
            return;
        }
        self.ask_for_a_list(tracks);
    }

    pub fn play_list(&mut self, at: usize) -> Result<()> {
        let Some((list, _)) = self.lists.get(at) else {
            return Ok(());
        };
        let (id, name) = (list.id, list.name.clone());
        let tracks = self.db.get_playlist_tracks(id)?;
        let Some(first) = tracks.first() else {
            self.set_status(tr!("„{}“ ist leer", name));
            return Ok(());
        };
        let _ = self.db.record_play(first.id);
        self.add_and_play(tracks, &name)?;
        Ok(())
    }

    pub fn list_into_queue(&mut self, at: usize) -> Result<()> {
        let Some((list, _)) = self.lists.get(at) else {
            return Ok(());
        };
        let (id, name) = (list.id, list.name.clone());
        let tracks = self.db.get_playlist_tracks(id)?;
        if tracks.is_empty() {
            self.set_status(tr!("„{}“ ist leer", name));
            return Ok(());
        }
        let held = tracks.len();
        self.player.play_next(tracks)?;
        self.set_status(tr!("{} titel aus „{}“ eingereiht",
            text::count(held as u64),
            name
        ));
        self.show_the_queue();
        Ok(())
    }

    pub fn go_to_lists(&mut self) -> Result<()> {
        self.use_device(false)?;
        self.open_lists();
        self.focus = Column::Lists;
        self.scroll_to = Some(Column::Lists);
        let standing = self.editing.and_then(|id| self.list_at(id));
        let Some(next) = list_after(standing, self.lists.len()) else {
            self.lists_index = 0;
            self.set_status(tr!("noch keine listen — enter legt eine an"));
            return Ok(());
        };
        self.lists_index = next;
        self.edit_list(next)
    }

    pub fn edit_list(&mut self, at: usize) -> Result<()> {
        let Some((list, _)) = self.lists.get(at) else {
            return Ok(());
        };
        let (id, name) = (list.id, list.name.clone());
        if self.editing != Some(id) {
            self.editing_tracks = self.db.get_playlist_tracks(id)?;
            self.editing = Some(id);
            self.queue_index = 0;
        }
        self.lists_last = Some(id);
        self.lists_index = at;
        self.set_status(match self.editing_tracks.len() {
            0 => tr!("„{}“ ist offen — titel hierher ziehen", name),
            n => tr!("„{}“ ist offen, {} titel", name, text::count(n as u64)),
        });
        self.open_lists();
        Ok(())
    }

    pub fn stop_editing(&mut self) {
        self.editing = None;
        self.editing_tracks = Vec::new();
        self.queue_index = 0;
        if self.focus == Column::Queue && self.lists_open() {
            self.focus = Column::Lists;
        }
    }

    pub fn editing_name(&self) -> Option<&str> {
        let id = self.editing?;
        self.lists
            .iter()
            .find(|(list, _)| list.id == id)
            .map(|(list, _)| list.name.as_str())
    }

    pub(in crate::gui) fn write_editing(&mut self) -> Result<()> {
        let Some(id) = self.editing else {
            return Ok(());
        };
        let ids: Vec<i64> = self.editing_tracks.iter().map(|t| t.id).collect();
        self.db.set_playlist_tracks(id, &ids)?;
        self.read_lists()
    }

    pub fn drop_into_list(&mut self, at: usize) -> Result<bool> {
        let Some(carried) = self.dragging.take() else {
            return Ok(false);
        };
        match carried {
            Dragged::Queued(from) => {
                if from >= self.editing_tracks.len() {
                    return Ok(false);
                }
                let to = landed(from, at);
                let row = self.editing_tracks.remove(from);
                self.editing_tracks.insert(to.min(self.editing_tracks.len()), row);
                self.queue_index = to.min(self.editing_tracks.len().saturating_sub(1));
            }
            Dragged::Episodes(_) | Dragged::QueuedEpisode(_) => return Ok(false),
            Dragged::Incoming(tracks) => {
                self.keep_the_peek();
                let count = tracks.len();
                let at = at.min(self.editing_tracks.len());
                for (step, track) in tracks.into_iter().enumerate() {
                    self.editing_tracks.insert(at + step, track);
                }
                self.queue_index = at;
                let name = self.editing_name().unwrap_or(tr!("die liste")).to_string();
                self.set_status(match count {
                    1 => tr!("1 titel in „{}“", name),
                    n => tr!("{} titel in „{}“", text::count(n as u64), name),
                });
            }
        }
        self.write_editing()?;
        Ok(true)
    }

    pub fn drop_from_list(&mut self, at: usize) -> Result<()> {
        if at >= self.editing_tracks.len() {
            return Ok(());
        }
        let gone = self.editing_tracks.remove(at);
        self.queue_index = at.min(self.editing_tracks.len().saturating_sub(1));
        self.set_status(tr!("„{}“ aus der liste genommen",
            gone.display_title()
        ));
        self.write_editing()
    }

    pub fn play_from_list(&mut self, at: usize) -> Result<()> {
        if self.editing_tracks.is_empty() {
            return Ok(());
        }
        let at = at.min(self.editing_tracks.len() - 1);
        if let Some(track) = self.editing_tracks.get(at) {
            let _ = self.db.record_play(track.id);
        }
        let tracks = self.editing_tracks.clone();
        self.queue_index = at;
        self.focus = Column::Queue;
        timed("eine liste ab einem titel anspielen", || {
            self.player.play_tracks(tracks, at)
        })
    }
}
