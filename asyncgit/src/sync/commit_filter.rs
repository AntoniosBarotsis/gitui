use super::{commit_files::get_commit_diff, CommitId};
use crate::error::Result;
use bitflags::bitflags;
use fuzzy_matcher::FuzzyMatcher;
use git2::{Diff, Repository};
use std::sync::Arc;

///
pub type SharedCommitFilterFn = Arc<
	Box<dyn Fn(&Repository, &CommitId) -> Result<bool> + Send + Sync>,
>;

///
pub fn diff_contains_file(file_path: String) -> SharedCommitFilterFn {
	Arc::new(Box::new(
		move |repo: &Repository,
		      commit_id: &CommitId|
		      -> Result<bool> {
			let diff = get_commit_diff(
				repo,
				*commit_id,
				Some(file_path.clone()),
				None,
				None,
			)?;

			let contains_file = diff.deltas().len() > 0;

			Ok(contains_file)
		},
	))
}

bitflags! {
	///
	pub struct SearchFields: u32 {
		///
		const MESSAGE = 1 << 0;
		///
		const FILENAMES = 1 << 1;
		///
		const AUTHORS = 1 << 2;
		//TODO:
		// const COMMIT_HASHES = 1 << 3;
		// ///
		// const DATES = 1 << 4;
		// ///
		// const DIFFS = 1 << 5;
	}
}

impl Default for SearchFields {
	fn default() -> Self {
		Self::MESSAGE
	}
}

bitflags! {
	///
	pub struct SearchOptions: u32 {
		///
		const CASE_SENSITIVE = 1 << 0;
		///
		const FUZZY_SEARCH = 1 << 1;
	}
}

impl Default for SearchOptions {
	fn default() -> Self {
		Self::empty()
	}
}

///
#[derive(Default, Debug, Clone)]
pub struct LogFilterSearchOptions {
	///
	pub search_pattern: String,
	///
	pub fields: SearchFields,
	///
	pub options: SearchOptions,
}

///
#[derive(Default)]
pub struct LogFilterSearch {
	///
	pub matcher: fuzzy_matcher::skim::SkimMatcherV2,
	///
	pub options: LogFilterSearchOptions,
}

impl LogFilterSearch {
	///
	pub fn new(options: LogFilterSearchOptions) -> Self {
		let mut options = options;
		if !options.options.contains(SearchOptions::CASE_SENSITIVE) {
			options.search_pattern =
				options.search_pattern.to_lowercase();
		}
		Self {
			matcher: fuzzy_matcher::skim::SkimMatcherV2::default(),
			options,
		}
	}

	fn match_diff(&self, diff: &Diff<'_>) -> bool {
		diff.deltas().any(|delta| {
			if delta
				.new_file()
				.path()
				.and_then(|file| file.as_os_str().to_str())
				.map(|file| self.match_text(file))
				.unwrap_or_default()
			{
				return true;
			}

			delta
				.old_file()
				.path()
				.and_then(|file| file.as_os_str().to_str())
				.map(|file| self.match_text(file))
				.unwrap_or_default()
		})
	}

	///
	pub fn match_text(&self, text: &str) -> bool {
		if self.options.options.contains(SearchOptions::FUZZY_SEARCH)
		{
			self.matcher
				.fuzzy_match(
					text,
					self.options.search_pattern.as_str(),
				)
				.is_some()
		} else if self
			.options
			.options
			.contains(SearchOptions::CASE_SENSITIVE)
		{
			text.contains(self.options.search_pattern.as_str())
		} else {
			text.to_lowercase()
				.contains(self.options.search_pattern.as_str())
		}
	}
}

///
pub fn filter_commit_by_search(
	filter: LogFilterSearch,
) -> SharedCommitFilterFn {
	Arc::new(Box::new(
		move |repo: &Repository,
		      commit_id: &CommitId|
		      -> Result<bool> {
			let commit = repo.find_commit((*commit_id).into())?;

			let msg_match = filter
				.options
				.fields
				.contains(SearchFields::MESSAGE)
				.then(|| {
					commit.message().map(|msg| filter.match_text(msg))
				})
				.flatten()
				.unwrap_or_default();

			let file_match = filter
				.options
				.fields
				.contains(SearchFields::FILENAMES)
				.then(|| {
					get_commit_diff(
						repo, *commit_id, None, None, None,
					)
					.ok()
				})
				.flatten()
				.map(|diff| filter.match_diff(&diff))
				.unwrap_or_default();

			let authors_match = filter
				.options
				.fields
				.contains(SearchFields::AUTHORS)
				.then(|| {
					let name_match = commit
						.author()
						.name()
						.map(|name| filter.match_text(name))
						.unwrap_or_default();
					let mail_match = commit
						.author()
						.email()
						.map(|name| filter.match_text(name))
						.unwrap_or_default();

					name_match || mail_match
				})
				.unwrap_or_default();

			Ok(msg_match || file_match || authors_match)
		},
	))
}
