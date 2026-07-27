//! Filesystem-backed MiniJinja environments.

use std::borrow::Cow;
use std::io;
use std::path::{Component, Path, PathBuf};
use std::sync::Arc;

use minijinja::{Environment, Error, ErrorKind};

/// Builds a cached filesystem-only environment and validates its entry template.
pub fn environment<R>(
    entry: PathBuf,
    home: Option<PathBuf>,
    read: R,
) -> Result<(Environment<'static>, String), Error>
where
    R: Fn(&Path) -> io::Result<String> + Send + Sync + 'static,
{
    let (environment, entry_name) = environment_unchecked(entry, home, read)?;
    environment.get_template(&entry_name)?;
    Ok((environment, entry_name))
}

/// Builds an environment without loading the entry so callers can recover from initial errors.
pub(crate) fn environment_unchecked<R>(
    entry: PathBuf,
    home: Option<PathBuf>,
    read: R,
) -> Result<(Environment<'static>, String), Error>
where
    R: Fn(&Path) -> io::Result<String> + Send + Sync + 'static,
{
    let home = Arc::new(home);
    let entry = expand_home(&entry, home.as_deref())?;
    let entry_name = entry.to_string_lossy().into_owned();
    let mut environment = Environment::new();
    let loader_home = Arc::clone(&home);
    environment.set_loader(move |name| {
        let path = expand_home(Path::new(name), loader_home.as_deref())?;
        match read(&path) {
            Ok(source) => Ok(Some(source)),
            Err(error) if error.kind() == io::ErrorKind::NotFound => Ok(None),
            Err(error) => Err(Error::new(
                ErrorKind::InvalidOperation,
                format!("could not read template {}", path.display()),
            )
            .with_source(error)),
        }
    });
    let join_home = Arc::clone(&home);
    environment.set_path_join_callback(move |name, parent| {
        let name = Path::new(name);
        let path = if name.is_absolute() || name.starts_with("~") {
            expand_home(name, join_home.as_deref()).unwrap_or_else(|_| name.to_path_buf())
        } else {
            Path::new(parent)
                .parent()
                .unwrap_or_else(|| Path::new(""))
                .join(name)
        };
        Cow::Owned(normalize(path).to_string_lossy().into_owned())
    });
    Ok((environment, entry_name))
}

fn expand_home(path: &Path, home: Option<&Path>) -> Result<PathBuf, Error> {
    let mut components = path.components();
    if components
        .next()
        .is_some_and(|part| part.as_os_str() == "~")
    {
        let home = home.ok_or_else(|| {
            Error::new(
                ErrorKind::InvalidOperation,
                "cannot expand template path without a home directory",
            )
        })?;
        return Ok(normalize(home.join(components.as_path())));
    }
    Ok(normalize(path.to_path_buf()))
}

fn normalize(path: PathBuf) -> PathBuf {
    let mut normalized = PathBuf::new();
    for component in path.components() {
        match component {
            Component::CurDir => {},
            Component::ParentDir => {
                normalized.pop();
            },
            component => normalized.push(component.as_os_str()),
        }
    }
    normalized
}

#[cfg(test)]
mod tests {
    use std::collections::BTreeMap;
    use std::io;
    use std::path::{Path, PathBuf};
    use std::sync::{Arc, Mutex};

    use super::environment;

    fn reader(
        files: BTreeMap<PathBuf, &'static str>,
        reads: Arc<Mutex<Vec<PathBuf>>>,
    ) -> impl Fn(&Path) -> io::Result<String> + Send + Sync + 'static {
        move |path| {
            reads.lock().unwrap().push(path.to_path_buf());
            files
                .get(path)
                .map(|source| source.to_string())
                .ok_or_else(|| io::Error::new(io::ErrorKind::NotFound, "missing template"))
        }
    }

    #[test]
    fn loads_entry_and_relative_includes_once() {
        let reads = Arc::new(Mutex::new(Vec::new()));
        let files = BTreeMap::from([
            (
                PathBuf::from("/templates/main.jinja"),
                "{% include 'parts/tab.jinja' %}",
            ),
            (PathBuf::from("/templates/parts/tab.jinja"), "tab"),
        ]);
        let (environment, entry) = environment(
            PathBuf::from("/templates/main.jinja"),
            None,
            reader(files, Arc::clone(&reads)),
        )
        .unwrap();

        assert_eq!(
            environment
                .get_template(&entry)
                .unwrap()
                .render(())
                .unwrap(),
            "tab"
        );
        assert_eq!(
            environment
                .get_template(&entry)
                .unwrap()
                .render(())
                .unwrap(),
            "tab"
        );
        assert_eq!(
            *reads.lock().unwrap(),
            [
                PathBuf::from("/templates/main.jinja"),
                PathBuf::from("/templates/parts/tab.jinja"),
            ]
        );
    }

    #[test]
    fn expands_home_in_entry_and_include_paths() {
        let reads = Arc::new(Mutex::new(Vec::new()));
        let files = BTreeMap::from([
            (
                PathBuf::from("/home/q/main.jinja"),
                "{% include '~/shared.jinja' %}",
            ),
            (PathBuf::from("/home/q/shared.jinja"), "shared"),
        ]);
        let (environment, entry) = environment(
            PathBuf::from("~/main.jinja"),
            Some(PathBuf::from("/home/q")),
            reader(files, Arc::clone(&reads)),
        )
        .unwrap();

        assert_eq!(
            environment
                .get_template(&entry)
                .unwrap()
                .render(())
                .unwrap(),
            "shared"
        );
    }
}
