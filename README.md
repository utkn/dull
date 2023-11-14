## Dull
A dull dotfile manager

### Installation
Clone this repository, `cd` into it and invoke `cargo` to build and install it for you:
```bash
$ cargo install
```

### Quick Start
Create a new folder. The goal is to set up `dull` such that all your dotfiles can be managed from here. 
Create a configuration file `config.toml` at the root. The configuration file should contain `module`s that map a `source` path to a `target` path. In this example, we will assume that we want to manage three configurations (i.e., `alacritty`, `helix`, and `fish`) and have the following folder structure:
```
.
├── config.toml
└── modules
    ├── alacritty
    │   └── alacritty.yml
    ├── fish
    │   ├── config.fish
    │   ├── fish_variables
    │   └── functions
    │       └── fish_prompt.fish
    └── helix
        ├── config.toml
        └── themes
            └── custom.toml
```
Accordingly, the configuration file `config.toml` might contain the following:

```toml
# config.toml
[[module]]
source = "modules/alacritty"
target = "~/.config/alacritty"

[[module]]
source = "modules/fish"
target = "~/.config/fish"

[[module]]
source = "modules/helix"
target = "~/.config/helix"
```

First, we build the system:
```bash
$ dull build
```
This creates a virtual filesystem under the folder `./builds`. The build will fail if there are conflicting modules. 

Then, we deploy the latest build:
```bash
$ dull deploy
```

This creates symlinks in the target directories (e.g., `~/.config/alacritty/alacritty.yml` will point to `./modules/alacritty/alacritty.yml`) which allows the user to manage their configurations from a single directory, allowing them to be easily maintained with version control like `git`. 

Alternatively, you can perform a hard deploy which directly copies the files from the modules to their target paths:

```bash
$ dull deploy --hard
```
This makes sense when you want to remove `dull` from your system.

To remove the deployed files, invoke:
```bash
$ dull undeploy
```

#### Directives
You can set *directive*s to limit the recursive linking. There are two possible directives: `linkthis`, and `linkthese`.

A `linkthis` directive can be added by creating a `.dull-linkthis` file under a folder in one of your modules. The folder containing this marker file will be linked directly. Similarly, a `linkthis` directive can be added by creating a `.dull-linkthese` file. All the files and folders that are in the same directory with this marker file will be linked directly.

These directives can alternatively be specified in the configuration file, instead of creating marker files as described above.
