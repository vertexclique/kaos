///
/// Macro to define a point to flunk
#[macro_export]
macro_rules! flunk {
    ($name:expr) => {{
        $crate::flunker($name, |_| {
            panic!("KAOS: Flunking at \"{}\"", $name);
        });
    }};
}

///
/// Define kaos tests
#[macro_export]
macro_rules! kaostest {
    ($name:expr, $body:block) => {{
        let scenario = $crate::Scene::setup();
        $crate::flunker_cfg($name, "panic").unwrap();

        $body

        scenario.teardown();
    }};
}


#[cfg(test)]
mod macro_tests {
    #[test]
    fn kaostest() {
        kaostest!("potato", {
            println!("potato");
        });
    }
}
