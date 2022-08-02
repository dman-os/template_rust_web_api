# > *rust-template-basic*

A starter template best suited for rust repositories that:

- are or have binary crates as opposed to libraries.
- have large dependency trees with long compilation times.

Features include:

## Dynamic linking

...for faster compilation times. 

> Note: before you commmit to this, be sure to give check if using the [`mold`][mold] linker alone can ease your compilation time pains. I've found that it is more than fast enough for most usecases, especially with how hacky and painful this template can get. This template itself is configured to use [`lld`][lld], which `mold` hooks into. This can be disable by commenting out marked sections of `.cargo/config.toml`.

This *hack* is a modification of the one used in the [`bevy`][bevy] repository and is implemented as following:

* External dependencies are pulled through a crate found at `crates/deps`.
*  A dummy `dylinb` crate at `crates/dylink` which also relies on `deps`. 
*  Our binary crate, which in this template is also the root of the workspace, relies on both `deps` and `dylink`.
   -  The `dylink` is configured as optional and removing it from the default features is all needed to disable dynamic linking.


This introduces a lot of...complications. Namely:

* You'll have to have `use deps::*;` at the top of each source file in order to access the dependencies.
  - This is can be used as a local prelude of sorts. All items you expose from `crates/deps/lib.rs` will awlays be availaible.
* Some dependencies will malfunction, usually their procedural macros. This occurs with some approaches for source crate resolution isn't amenable for re-exporting.
   - Sometimes there'll be a workaround for them like for example:
       + Serde exposes an API for changing what name it's macros resolve to. 
         * Add the atttribute `#[serde(crate = "serde")]` to your type declarations.
       + Bevy macros resolve their crates at the `call_site`. Adding the following line the import statements or better yet, to `crates/deps/lib.rs` to avoid deduplication resolves such cases:
           * `pub use bevy::{ecs as bevy_ecs, reflect as bevy_reflect};`
           * Use more `_ as _` expresssions if bevy exports macros from other sub crates.
   - Some macros, like those in [`sqlx`][sqlx], are not re-exportable.
       + Disabling dynamic linking will solve all issues in that case.
           * Remove `dylink` from the default features in the root `Cargo.toml` manifest.
       + If you really have bad compile times, you can move where the troublsome crates are specified from `crates/deps/Cargo.toml` to the root `Cargo.toml`. Some notices:
           * If the crate has a lot of dependencies, they all won't be dynamically linked. 
             - If this is the crate responsible for the bad compilation times in the first place, this renders all this voodoo moot.
           * If you have additional crates in your workspace that rely on this dependency, you'll need to specify it in their manifests as well.
           * It's ugly.
* You won't be able to run your builds directly directly as `target/debug/binname`. 
    - You might want to do this for scripts or running with root permissions.
    - Using `LD_LIBRARY_PATH` to provide the path to the rust std lib and your `dylink` lib should solve this:
      +  ```$ env LD_LIBRARY_PATH="{{pwd}}/target/debug:{{RUSTUP_CUR_TOOLCHAIN_PATH}}/lib/"  ./target/debug/binname```
        * On linux, `echo` the following oneliner to programmatically find the path to your std shared objects:
          - ```$ `rustup show home`"/toolchains/"`rustup show active-toolchain | rg '(.+) \(.+\)' -r '$1'`"/lib/"```
            +  TODO: replace `rg` with `grep` here.
       * TODO: simlar one liner for windows.

If you have build time machinations, especially generative ones, you should be able to specify them at the `deps` crate and should work as specified usually.

## Cargo XTASK stubs

...for xplatform scripting. This is implemented according to the [official-but-not-really](https://github.com/matklad/cargo-xtask) spec using [`clap`](https://lib.rs/crates/clap).

##  Common dependencies

...I usually include in most projects.

Be sure to run...
```
cargo upgrade --workspace --skip-compatible
``` 
...afterwards to make use of the latest versions.

That `upgrade` command is part of the [`cargo-edit`][cedit] extension. Do..

```
$ cargo install --locked cargo-edit
```

...to install it.

### Acknowledgements

- [Matthias Endler](https://endler.dev/about)'s [Tips for Faster Rust Compile Times](https://endler.dev/2020/rust-compile-times/).
- The [`bevy`][bevy] authors.

[lld]: https://lld.llvm.org/
[mold]: https://github.com/rui314/mold/
[bevy]: https://bevyengine.org/
[cedit]: https://lib.rs/crates/cargo-edit
[sqlx]: https://lib.rs/crates/sqlx