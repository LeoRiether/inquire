use crossterm::event::KeyModifiers;
use std::{collections::HashSet, iter::FromIterator};
use unicode_segmentation::UnicodeSegmentation;

use crate::{
    answer::OptionAnswer,
    config::{self, Filter},
    error::{InquireError, InquireResult},
    formatter::{self, MultiOptionFormatter},
    key::Key,
    renderer::Renderer,
    terminal::Terminal,
    utils::paginate,
    validator::MultiOptionValidator,
};

/// Presents a message to the user and a list of options for the user to choose from.
/// The user is able to choose multiple options.
#[derive(Copy, Clone)]
pub struct MultiSelect<'a> {
    /// Message to be presented to the user.
    pub message: &'a str,

    /// Options displayed to the user.
    pub options: &'a [&'a str],

    /// Default indexes of options to be selected from the start.
    pub default: Option<&'a [usize]>,

    /// Help message to be presented to the user.
    pub help_message: Option<&'a str>,

    /// Page size of the options displayed to the user.
    pub page_size: usize,

    /// Whether vim mode is enabled. When enabled, the user can
    /// navigate through the options using hjkl.
    pub vim_mode: bool,

    /// Starting cursor index of the selection.
    pub starting_cursor: usize,

    /// Function called with the current user input to filter the provided
    /// options.
    pub filter: Filter,

    /// Whether the current filter typed by the user is kept or cleaned after a selection is made.
    pub keep_filter: bool,

    /// Function that formats the user input and presents it to the user as the final rendering of the prompt.
    pub formatter: MultiOptionFormatter,

    /// Validator to apply to the user input.
    pub validator: Option<MultiOptionValidator<'a>>,
}

impl<'a> MultiSelect<'a> {
    /// Default formatter that maps the collection of options to their string values and joins them using a comma as the separator.
    pub const DEFAULT_FORMATTER: MultiOptionFormatter = formatter::DEFAULT_MULTI_OPTION_FORMATTER;
    /// Default filter, equal to the global default filter [config::DEFAULT_FILTER]
    pub const DEFAULT_FILTER: Filter = config::DEFAULT_FILTER;
    /// Default page size, equal to the global default page size [config::DEFAULT_PAGE_SIZE]
    pub const DEFAULT_PAGE_SIZE: usize = config::DEFAULT_PAGE_SIZE;
    /// Default value of vim mode, equal to the global default value [config::DEFAULT_PAGE_SIZE]
    pub const DEFAULT_VIM_MODE: bool = config::DEFAULT_VIM_MODE;
    /// Default starting cursor index.
    pub const DEFAULT_STARTING_CURSOR: usize = 0;
    /// Default behavior of keeping or cleaning the current filter value.
    pub const DEFAULT_KEEP_FILTER: bool = true;
    /// Default help message.
    pub const DEFAULT_HELP_MESSAGE: Option<&'a str> =
        Some("↑↓ to move, space to select one, → to all, ← to none, type to filter");

    /// Creates a [MultiSelect] with the provided message and options, along with default configuration values.
    pub fn new(message: &'a str, options: &'a [&str]) -> Self {
        Self {
            message,
            options,
            default: None,
            help_message: Self::DEFAULT_HELP_MESSAGE,
            page_size: Self::DEFAULT_PAGE_SIZE,
            vim_mode: Self::DEFAULT_VIM_MODE,
            starting_cursor: Self::DEFAULT_STARTING_CURSOR,
            keep_filter: Self::DEFAULT_KEEP_FILTER,
            filter: Self::DEFAULT_FILTER,
            formatter: Self::DEFAULT_FORMATTER,
            validator: None,
        }
    }

    /// Sets the help message of the prompt.
    pub fn with_help_message(mut self, message: &'a str) -> Self {
        self.help_message = Some(message);
        self
    }

    /// Removes the set help message.
    pub fn without_help_message(mut self) -> Self {
        self.help_message = None;
        self
    }

    /// Sets the page size.
    pub fn with_page_size(mut self, page_size: usize) -> Self {
        self.page_size = page_size;
        self
    }

    /// Enables or disabled vim_mode.
    pub fn with_vim_mode(mut self, vim_mode: bool) -> Self {
        self.vim_mode = vim_mode;
        self
    }

    /// Sets the keep filter behavior.
    pub fn with_keep_filter(mut self, keep_filter: bool) -> Self {
        self.keep_filter = keep_filter;
        self
    }

    /// Sets the filter function.
    pub fn with_filter(mut self, filter: Filter) -> Self {
        self.filter = filter;
        self
    }

    /// Sets the formatter.
    pub fn with_formatter(mut self, formatter: MultiOptionFormatter) -> Self {
        self.formatter = formatter;
        self
    }

    /// Adds a validator to the collection of validators.
    pub fn with_validator(mut self, validator: MultiOptionValidator<'a>) -> Self {
        self.validator = Some(validator);
        self
    }

    /// Sets the indexes to be selected by the default.
    pub fn with_default(mut self, default: &'a [usize]) -> Self {
        self.default = Some(default);
        self
    }

    /// Sets the starting cursor index.
    pub fn with_starting_cursor(mut self, starting_cursor: usize) -> Self {
        self.starting_cursor = starting_cursor;
        self
    }

    /// Parses the provided behavioral and rendering options and prompts
    /// the CLI user for input according to them.
    pub fn prompt(self) -> InquireResult<Vec<OptionAnswer>> {
        let terminal = Terminal::new()?;
        let mut renderer = Renderer::new(terminal)?;
        self.prompt_with_renderer(&mut renderer)
    }

    pub(in crate) fn prompt_with_renderer(
        self,
        renderer: &mut Renderer,
    ) -> InquireResult<Vec<OptionAnswer>> {
        MultiSelectPrompt::new(self)?.prompt(renderer)
    }
}

struct MultiSelectPrompt<'a> {
    message: &'a str,
    options: &'a [&'a str],
    help_message: Option<&'a str>,
    vim_mode: bool,
    cursor_index: usize,
    checked: HashSet<usize>,
    page_size: usize,
    keep_filter: bool,
    filter_value: Option<String>,
    filtered_options: Vec<usize>,
    filter: Filter,
    formatter: MultiOptionFormatter,
    validator: Option<MultiOptionValidator<'a>>,
    error: Option<String>,
}

impl<'a> MultiSelectPrompt<'a> {
    fn new(mso: MultiSelect<'a>) -> InquireResult<Self> {
        if mso.options.is_empty() {
            return Err(InquireError::InvalidConfiguration(
                "Available options can not be empty".into(),
            ));
        }
        if let Some(default) = mso.default {
            for i in default {
                if i >= &mso.options.len() {
                    return Err(InquireError::InvalidConfiguration(format!(
                        "Index {} is out-of-bounds for length {} of options",
                        i,
                        &mso.options.len()
                    )));
                }
            }
        }

        Ok(Self {
            message: mso.message,
            options: mso.options,
            help_message: mso.help_message,
            vim_mode: mso.vim_mode,
            cursor_index: mso.starting_cursor,
            page_size: mso.page_size,
            keep_filter: mso.keep_filter,
            filter_value: None,
            filtered_options: Vec::from_iter(0..mso.options.len()),
            filter: mso.filter,
            formatter: mso.formatter,
            validator: mso.validator,
            error: None,
            checked: mso
                .default
                .map_or_else(|| HashSet::new(), |d| d.iter().cloned().collect()),
        })
    }

    fn filter_options(&self) -> Vec<usize> {
        self.options
            .iter()
            .enumerate()
            .filter_map(|(i, opt)| match &self.filter_value {
                Some(val) if (self.filter)(&val, opt, i) => Some(i),
                Some(_) => None,
                None => Some(i),
            })
            .collect()
    }

    fn move_cursor_up(&mut self) {
        self.cursor_index = self
            .cursor_index
            .checked_sub(1)
            .or(self.filtered_options.len().checked_sub(1))
            .unwrap_or_else(|| 0);
    }

    fn move_cursor_down(&mut self) {
        self.cursor_index = self.cursor_index.saturating_add(1);
        if self.cursor_index >= self.filtered_options.len() {
            self.cursor_index = 0;
        }
    }

    fn toggle_cursor_selection(&mut self) {
        let idx = match self.filtered_options.get(self.cursor_index) {
            Some(val) => val,
            None => return,
        };

        if self.checked.contains(idx) {
            self.checked.remove(idx);
        } else {
            self.checked.insert(*idx);
        }

        if !self.keep_filter {
            self.filter_value = None;
        }
    }

    fn on_change(&mut self, key: Key) {
        let old_filter = self.filter_value.clone();

        match key {
            Key::Up(KeyModifiers::NONE) => self.move_cursor_up(),
            Key::Char('k', KeyModifiers::NONE) if self.vim_mode => self.move_cursor_up(),
            Key::Tab | Key::Down(KeyModifiers::NONE) => self.move_cursor_down(),
            Key::Char('j', KeyModifiers::NONE) if self.vim_mode => self.move_cursor_down(),
            Key::Char(' ', KeyModifiers::NONE) => self.toggle_cursor_selection(),
            Key::Backspace => {
                if let Some(filter) = &self.filter_value {
                    let len = filter[..].graphemes(true).count();
                    let new_len = len.saturating_sub(1);
                    self.filter_value = Some(filter[..].graphemes(true).take(new_len).collect());
                }
            }
            Key::Right(KeyModifiers::NONE) => {
                self.checked.clear();
                for idx in &self.filtered_options {
                    self.checked.insert(*idx);
                }

                if !self.keep_filter {
                    self.filter_value = None;
                }
            }
            Key::Left(KeyModifiers::NONE) => {
                self.checked.clear();

                if !self.keep_filter {
                    self.filter_value = None;
                }
            }
            Key::Char(c, KeyModifiers::NONE) => match &mut self.filter_value {
                Some(val) => val.push(c),
                None => self.filter_value = Some(String::from(c)),
            },
            _ => {}
        }

        if self.filter_value != old_filter {
            let options = self.filter_options();
            if options.len() > 0 && options.len() <= self.cursor_index {
                self.cursor_index = options.len().saturating_sub(1);
            }
            self.filtered_options = options;
        }
    }

    fn get_final_answer(&self) -> Result<Vec<OptionAnswer>, String> {
        let selected_options = self
            .options
            .iter()
            .enumerate()
            .filter_map(|(idx, opt)| match &self.checked.contains(&idx) {
                true => Some(OptionAnswer::new(idx, opt)),
                false => None,
            })
            .collect::<Vec<OptionAnswer>>();

        if let Some(validator) = self.validator {
            return match validator(&selected_options) {
                Ok(_) => Ok(selected_options),
                Err(err) => Err(err),
            };
        }

        return Ok(selected_options);
    }

    fn render(&mut self, renderer: &mut Renderer) -> InquireResult<()> {
        let prompt = &self.message;

        renderer.reset_prompt()?;

        if let Some(err) = &self.error {
            renderer.print_error_message(err)?;
        }

        renderer.print_prompt(&prompt, None, self.filter_value.as_deref())?;

        let choices = self
            .filtered_options
            .iter()
            .cloned()
            .map(|i| OptionAnswer::new(i, self.options.get(i).unwrap()))
            .collect::<Vec<OptionAnswer>>();

        let (paginated_opts, rel_sel) = paginate(self.page_size, &choices, self.cursor_index);

        for (idx, opt) in paginated_opts.iter().enumerate() {
            renderer.print_multi_option(
                rel_sel == idx,
                self.checked.contains(&opt.index),
                &opt.value,
            )?;
        }

        if let Some(help_message) = self.help_message {
            renderer.print_help(help_message)?;
        }

        renderer.flush()?;

        Ok(())
    }

    fn prompt(mut self, renderer: &mut Renderer) -> InquireResult<Vec<OptionAnswer>> {
        let final_answer: Vec<OptionAnswer>;

        loop {
            self.render(renderer)?;

            let key = renderer.read_key()?;

            match key {
                Key::Cancel => return Err(InquireError::OperationCanceled),
                Key::Submit => match self.get_final_answer() {
                    Ok(answer) => {
                        final_answer = answer;
                        break;
                    }
                    Err(err) => self.error = Some(err),
                },
                key => self.on_change(key),
            }
        }

        let formatted = (self.formatter)(&final_answer);

        renderer.cleanup(&self.message, &formatted)?;

        Ok(final_answer)
    }
}
