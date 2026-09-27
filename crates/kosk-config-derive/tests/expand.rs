//! Expansion test for the derives against a stub schema mirroring
//! `crate::config::schema`.

use kosk_config_derive::{Choice, ConfigSection};

use crate::config::schema::{Choice as _, Lens, Page, Setting};
use crate::controller::ControllerKind;

mod controller {
    #[derive(Clone, Copy, PartialEq, Eq, Debug)]
    pub enum ControllerKind {
        Sc2,
        Ps4,
    }
}

mod config {
    pub mod schema {
        use std::sync::Arc;

        use crate::controller::ControllerKind;

        #[derive(Clone, Copy, PartialEq, Eq, Debug)]
        pub enum Page {
            Suggestions,
            Overlay,
            Device(ControllerKind, DevicePage),
        }

        #[derive(Clone, Copy, PartialEq, Eq, Debug)]
        pub enum DevicePage {
            Pads,
            Stick,
        }

        #[derive(Clone)]
        pub struct Lens<T> {
            read: Arc<dyn for<'a> Fn(&'a crate::Root) -> &'a T + Send + Sync>,
            write: Arc<dyn for<'a> Fn(&'a mut crate::Root) -> &'a mut T + Send + Sync>,
        }

        impl Lens<crate::Root> {
            pub fn root() -> Self {
                Self {
                    read: Arc::new(|c: &crate::Root| c),
                    write: Arc::new(|c: &mut crate::Root| c),
                }
            }
        }

        impl<T: 'static> Lens<T> {
            pub fn field<U: 'static>(
                &self,
                get: fn(&T) -> &U,
                get_mut: fn(&mut T) -> &mut U,
            ) -> Lens<U> {
                let read = Arc::clone(&self.read);
                let write = Arc::clone(&self.write);
                Lens {
                    read: Arc::new(move |cfg| get(read(cfg))),
                    write: Arc::new(move |cfg| get_mut(write(cfg))),
                }
            }
        }

        pub struct Setting {
            pub key: &'static str,
            pub page: Page,
            pub label: &'static str,
            pub explain: &'static str,
            pub advanced: bool,
            pub control: Box<dyn Control>,
        }

        pub trait Control: Send + Sync {
            fn probe(&self) -> &'static str;
        }

        pub struct BoolControl<T> {
            pub lens: Lens<T>,
        }
        pub struct NumControl<T> {
            pub lens: Lens<T>,
            pub min: T,
            pub max: T,
            pub step: T,
            pub digits: u32,
            pub unit: Option<&'static str>,
        }
        pub struct ChoiceControl<T> {
            pub lens: Lens<T>,
        }
        pub struct MirrorChoiceControl<T> {
            pub lens: Lens<T>,
            pub mirror: Lens<T>,
        }

        impl<T: Send + Sync + 'static> Control for BoolControl<T> {
            fn probe(&self) -> &'static str {
                "bool"
            }
        }
        impl<T: Send + Sync + 'static> Control for NumControl<T> {
            fn probe(&self) -> &'static str {
                "num"
            }
        }
        impl<T: Send + Sync + 'static> Control for ChoiceControl<T> {
            fn probe(&self) -> &'static str {
                "choice"
            }
        }
        impl<T: Send + Sync + 'static> Control for MirrorChoiceControl<T> {
            fn probe(&self) -> &'static str {
                "mirror"
            }
        }

        pub trait Choice: Clone + PartialEq + Send + Sync + 'static {
            const ALL: &'static [Self];
            fn label(&self) -> &'static str;
        }

        pub fn leak_key(key: String) -> &'static str {
            Box::leak(key.into_boxed_str())
        }
    }
}

#[derive(Choice, Clone, Copy, PartialEq, Eq, Debug)]
enum Flavor {
    #[choice(label = "Smart")]
    Ngram,
    Dictionary,
    AboveField,
}

#[derive(ConfigSection, Debug, PartialEq)]
struct Inner {
    #[config(default = 3.8)]
    #[setting(
        label = "Range",
        explain = "How far it reaches.",
        range = 1.0..=8.0,
        step = 0.1,
        decimals = 1
    )]
    scale: f32,
    #[config(default = true)]
    #[setting(label = "Flag", explain = "Toggles it.", advanced)]
    flag: bool,
    #[config(default = Flavor::Ngram)]
    #[setting(page = Overlay, label = "Flavor", explain = "Picks it.")]
    flavor: Flavor,
}

#[derive(ConfigSection, Debug, PartialEq)]
struct Root {
    #[setting(section, page = Suggestions)]
    inner: Inner,
    #[setting(section, page = Device(Sc2, Pads))]
    pad: Inner,
    #[config(default = 7u8)]
    #[setting(
        page = Overlay,
        label = "Level",
        explain = "How much.",
        range = 0..=255,
        step = 5
    )]
    level: u8,
    plain: u64,
}

#[test]
fn defaults_come_from_config_attributes() {
    let root = Root::default();
    assert_eq!(root.inner.scale, 3.8);
    assert!(root.inner.flag);
    assert_eq!(root.inner.flavor, Flavor::Ngram);
    assert_eq!(root.level, 7);
    assert_eq!(root.plain, 0);
    assert_eq!(Root::__sdef_level(), 7);
    assert_eq!(Inner::__sdef_scale(), 3.8);
}

#[test]
fn collect_builds_prefixed_settings() {
    use crate::config::schema::DevicePage;

    let mut out: Vec<Setting> = Vec::new();
    Root::__kosk_collect(&Lens::root(), None, String::new(), &mut out);
    // `plain` has no setting attribute, so it is skipped.
    assert_eq!(out.len(), 7);
    assert_eq!(out[0].key, "inner.scale");
    assert_eq!(out[0].page, Page::Suggestions);
    assert_eq!(out[0].label, "Range");
    assert_eq!(out[0].control.probe(), "num");
    assert!(out[1].advanced);
    assert_eq!(out[1].control.probe(), "bool");
    // An explicit leaf page wins over the inherited section page.
    assert_eq!(out[2].control.probe(), "choice");
    assert_eq!(out[2].page, Page::Overlay);
    assert_eq!(out[3].key, "pad.scale");
    assert_eq!(
        out[3].page,
        Page::Device(ControllerKind::Sc2, DevicePage::Pads)
    );
    assert_eq!(out[6].key, "level");
    assert_eq!(out[6].page, Page::Overlay);
}

#[test]
fn choice_labels_and_order() {
    assert_eq!(Flavor::Ngram.label(), "Smart");
    assert_eq!(Flavor::Dictionary.label(), "Dictionary");
    assert_eq!(Flavor::AboveField.label(), "Above field");
    assert_eq!(
        Flavor::ALL,
        &[Flavor::Ngram, Flavor::Dictionary, Flavor::AboveField]
    );
}
