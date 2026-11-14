/// Declare the diagnostic codes: the one place a code exists (RFC-0129 D1).
///
/// ```text
/// diagnostics! {
///     /// What the code means, for a reader of this file.
///     0001 AmentPrefixUnset {
///         title: "one line saying what went wrong",
///         surfaces: [Cli],
///         remedy: "the default help line",
///     }
///     0007 Retired(OldName) => 0001,
/// }
/// ```
///
/// A live row becomes a [`Diag`](crate::Diag) variant whose discriminant is
/// its number, and a row of [`ALL`](crate::ALL) whose explanation is
/// `include_str!("../explain/NROS-NNNN.md")`: a live row without that file is
/// a compile error. A retired row keeps its number in `ALL`, names its
/// successor, and has no variant, so nothing can construct it. Rows are
/// written in ascending number order; a test holds that.
///
/// A row whose explanation file is missing does not compile:
///
/// ```compile_fail
/// nros_diagnostic_codes::diagnostics! {
///     9999 NoSuchExplanation {
///         title: "t",
///         surfaces: [Cli],
///         remedy: "r",
///     }
/// }
/// ```
#[macro_export]
macro_rules! diagnostics {
    // Done: emit the enum and the table.
    (@rows [$($variant:tt)*] [$($row:tt)*]) => {
        /// Every live diagnostic. The discriminant is the code's number.
        #[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
        #[repr(u16)]
        // Codes are written as four digits, `0412`: decimal, as the code is.
        #[allow(clippy::zero_prefixed_literal)]
        pub enum Diag { $($variant)* }

        /// Every code ever allocated, live and retired, in number order.
        #[allow(clippy::zero_prefixed_literal)]
        pub const ALL: &[$crate::Entry] = &[ $($row)* ];
    };

    // A retired row: the number stays, the variant goes.
    (@rows [$($variant:tt)*] [$($row:tt)*]
        $(#[$meta:meta])*
        $num:literal Retired($name:ident) => $succ:literal,
        $($rest:tt)*
    ) => {
        $crate::diagnostics!(@rows [$($variant)*] [$($row)*
            $crate::Entry {
                number: $num,
                code: concat!("NROS-", stringify!($num)),
                name: stringify!($name),
                title: "",
                surfaces: &[],
                remedy: "",
                explanation: "",
                retired: Some($succ),
            },
        ] $($rest)*);
    };

    // A live row.
    (@rows [$($variant:tt)*] [$($row:tt)*]
        $(#[$meta:meta])*
        $num:literal $name:ident {
            title: $title:expr,
            surfaces: [$($surface:ident),+ $(,)?],
            remedy: $remedy:expr $(,)?
        }
        $($rest:tt)*
    ) => {
        $crate::diagnostics!(@rows [$($variant)* $(#[$meta])* $name = $num,] [$($row)*
            $crate::Entry {
                number: $num,
                code: concat!("NROS-", stringify!($num)),
                name: stringify!($name),
                title: $title,
                surfaces: &[$($crate::Surface::$surface),+],
                remedy: $remedy,
                explanation: include_str!(concat!("../explain/NROS-", stringify!($num), ".md")),
                retired: None,
            },
        ] $($rest)*);
    };

    (@rows [$($variant:tt)*] [$($row:tt)*] $($bad:tt)*) => {
        compile_error!(concat!(
            "diagnostics!: cannot read this row; expected `NNNN Name { title, surfaces, remedy }` \
             or `NNNN Retired(Name) => NNNN,`, at: ",
            stringify!($($bad)*)
        ));
    };

    ($($body:tt)*) => {
        $crate::diagnostics!(@rows [] [] $($body)*);
    };
}
