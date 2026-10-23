//! The fixture libraries (`--demo` and `inject objects {"library":{"fixture":…}}`):
//! in memory, generated deterministically, fiction only — `--demo` never
//! reads the home.

use super::model::MemFs;

/// The demo tree: `projeto/{src,docs,testes}`, `notas.md`, `fotos/`, and a
/// fictional vault (`.ssh/{id_demo,config,known_hosts}`, `projeto/.env`).
pub fn demo() -> MemFs {
    let mut m = MemFs::new(1);
    m.text(
        0,
        "projeto/src/lib.rs",
        "//! O mundo dos gatos.\n\npub mod mundo;\n\npub fn ola() -> &'static str {\n    \"olá, mundo\"\n}\n",
    )
    .text(0, "projeto/src/main.rs", "fn main() {\n    println!(\"{}\", projeto::ola());\n}\n")
    .file(0, "projeto/src/mundo.rs", 4200)
    .file(0, "projeto/src/gatos.rs", 2800)
    .file(0, "projeto/src/estante.rs", 1900)
    .text(0, "projeto/docs/guia.md", "# Guia\n\nComo usar a biblioteca: h/l, j/k, enter.\n")
    .file(0, "projeto/docs/plano.md", 18_000)
    .file(0, "projeto/docs/diagrama.svg", 9_000)
    .file(0, "projeto/testes/lib_test.rs", 1_300)
    .file(0, "projeto/testes/estante_test.rs", 900)
    .text(0, "projeto/README.md", "# projeto\n\nUm projeto de demonstração.\n")
    .file(0, "projeto/Cargo.toml", 240)
    .file(0, "projeto/.env", 64)
    .file(0, "projeto/target/debug/projeto", 3_400_000)
    .text(
        0,
        "notas.md",
        "# notas\n\n- regar a planta da recepção\n- trocar o disco da vitrola\n- ler o guia da biblioteca\n",
    )
    .file(0, "fotos/praia.jpg", 2_400_000)
    .file(0, "fotos/gato-no-sol.png", 1_100_000)
    .file(0, "fotos/biblioteca.webp", 640_000)
    .file(0, "fotos/album.zip", 48_000_000)
    .file(0, "musica/acme-trilha.flac", 31_000_000)
    .file(0, "musica/bravo-tema.ogg", 4_200_000)
    .file(0, "receitas/bolo-de-cenoura.md", 1_400)
    .file(0, "receitas/pao-de-queijo.md", 1_100)
    .file(0, "receitas/cafe.txt", 300)
    .file(0, "livros/guia-do-mochileiro.epub", 690_000)
    .file(0, "livros/manual-alpha.pdf", 2_900_000)
    .file(0, "livros/contos-bravo.pdf", 1_300_000)
    .file(0, "api-gateway.tar.gz", 12_000_000)
    .file(0, "orcamento.csv", 7_000)
    .file(0, ".bashrc", 3_700)
    .file(0, ".ssh/id_demo", 411)
    .file(0, ".ssh/config", 120)
    .file(0, ".ssh/known_hosts", 940);
    for i in 1..=24 {
        m.file(0, &format!("livros/caderno-{i:02}.md"), 500 * i);
    }
    m
}

/// The `big` fixture: ≈ 50k entries, at most 12 deep, the same every time
/// (`S6.nks`, the search budget test).
pub fn big() -> MemFs {
    let mut m = MemFs::new(1);
    let exts = ["rs", "md", "txt", "png", "toml", "flac", "zip", "json"];
    for k in 0..2_500u32 {
        // a path of 1..=11 directories, from k's base-4 digits
        let depth = 1 + (k % 11) as usize;
        let mut dir = String::new();
        let mut x = k;
        for l in 0..depth {
            if l > 0 {
                dir.push('/');
            }
            dir.push_str(&format!("n{}{}", l, x % 4));
            x /= 4;
        }
        dir.push_str(&format!("-{k}"));
        for f in 0..19u32 {
            let ext = exts[((k + f) % exts.len() as u32) as usize];
            m.file(
                0,
                &format!("{dir}/arquivo{f:02}.{ext}"),
                (k * 37 + f * 101) as u64,
            );
        }
    }
    m
}

#[cfg(test)]
mod tests {
    use super::super::model::{Config, FsPort, read_shelf};
    use super::*;

    #[test]
    fn the_demo_tree_has_the_plan_s_names_and_three_vault_cards() {
        let m = demo();
        let c = Config::default();
        let s = read_shelf(&c, &m, 0, "").unwrap();
        let names: Vec<&str> = s.entries.iter().map(|e| e.name()).collect();
        for n in ["projeto", "fotos", "notas.md"] {
            assert!(names.contains(&n), "{n}");
        }
        assert!(!names.contains(&".ssh"));
        assert_eq!(s.vault_count, 1);
        let v = super::super::model::read_vault(&c, &m, 0, "").unwrap();
        let cards: Vec<&str> = v.entries.iter().map(|e| e.name()).collect();
        assert_eq!(cards, ["config", "id_demo", "known_hosts"]);
        let p = read_shelf(&c, &m, 0, "projeto").unwrap();
        let pn: Vec<&str> = p.entries.iter().map(|e| e.name()).collect();
        assert_eq!(&pn[..3], ["docs", "src", "testes"]);
        assert!(!pn.contains(&".env") && !pn.contains(&"target"));
    }

    #[test]
    fn big_is_about_50k_entries_at_most_12_deep_and_deterministic() {
        let b = big();
        let n = b.len();
        assert!((49_000..=56_000).contains(&n), "{n}");
        assert_eq!(n, big().len());
        let a = b.read_dir(0, "").unwrap();
        assert_eq!(a, big().read_dir(0, "").unwrap());
        fn depth(m: &MemFs, rel: &str) -> usize {
            m.read_dir(0, rel)
                .unwrap()
                .iter()
                .filter(|e| e.is_dir)
                .map(|e| 1 + depth(m, &e.rel))
                .max()
                .unwrap_or(0)
        }
        assert!(depth(&b, "") <= 11, "dirs ≤ 11 deep, files at ≤ 12");
    }
}
