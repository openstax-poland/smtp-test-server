// Copyright 2022 OpenStax Poland
// Licensed under the MIT license. See LICENSE file in the project root for
// full license text.

#[macro_export]
macro_rules! match_ignore_ascii_case {
    (@$value:expr; [$($arms:tt)*] _ => $default:expr $(,)?) => {
        $($arms)* {
            $default
        }
    };

    (@$value:expr; [$($arms:tt)*] $pat:literal $(| $apat:literal)* => { $($body:tt)* } $(,)? $($rest:tt)+) => {
        match_ignore_ascii_case!(@$value;
            [
                $($arms)*
                if $value.eq_ignore_ascii_case($pat) $(|| $value.eq_ignore_ascii_case($apat))* {
                    $($body)*
                } else
            ]
            $($rest)*
        )
    };

    (@$value:expr; [$($arms:tt)*] $pat:literal $(| $apat:literal)* => $arm:expr , $($rest:tt)+) => {
        match_ignore_ascii_case!(@$value;
            [
                $($arms)*
                if $value.eq_ignore_ascii_case($pat) $(|| $value.eq_ignore_ascii_case($apat))* {
                    $arm
                } else
            ]
            $($rest)*
        )
    };

    ($value:expr; $($rest:tt)*) => {
        match_ignore_ascii_case!(@$value; [] $($rest)*)
    }
}
