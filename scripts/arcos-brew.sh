prefix=/home/linuxbrew/.linuxbrew
brew_bin="$prefix/bin/brew"

if [ ! -x "$brew_bin" ]; then
  user="$(id -un)"
  group="$(id -gn)"
  printf 'Homebrew is installed on first use into %s.\n' "$prefix" >&2
  printf 'A system authorization prompt may appear once.\n' >&2
  pkexec install -d -m 0775 -o "$user" -g "$group" /home/linuxbrew "$prefix"
  if [ ! -d "$prefix/Homebrew/.git" ]; then
    git clone --filter=blob:none --branch 6.0.17 --single-branch \
      https://github.com/Homebrew/brew "$prefix/Homebrew"
  fi
  install -d "$prefix/bin"
  ln -sfn ../Homebrew/bin/brew "$brew_bin"
fi

export HOMEBREW_PREFIX="$prefix"
export HOMEBREW_CELLAR="$prefix/Cellar"
export HOMEBREW_REPOSITORY="$prefix/Homebrew"
exec "$brew_bin" "$@"
