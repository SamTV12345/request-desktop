//! `docker-run-to-docker-compose-converter`

use crate::*;
use crate::{Args, Category, Field, Kind, Tool, arg};
use anyhow::{Context, Result, bail};
use std::fmt::Write as _;

/// Split a shell-ish command line, honouring single and double quotes.
pub(crate) fn shell_split(input: &str) -> Result<Vec<String>> {
    let mut out = Vec::new();
    let mut cur = String::new();
    let mut quote: Option<char> = None;
    let mut started = false;
    for c in input.chars() {
        match (quote, c) {
            (Some(q), c) if c == q => quote = None,
            (Some(_), c) => cur.push(c),
            (None, '\'' | '"') => {
                quote = Some(c);
                started = true;
            }
            (None, c) if c.is_whitespace() => {
                if started {
                    out.push(std::mem::take(&mut cur));
                    started = false;
                }
            }
            (None, '\\') => {} // line continuations / escapes: drop
            (None, c) => {
                cur.push(c);
                started = true;
            }
        }
    }
    if quote.is_some() {
        bail!("unterminated quote");
    }
    if started {
        out.push(cur);
    }
    Ok(out)
}

/// ponytail: covers the docker run flags people actually paste. Anything else
/// is reported as "not translated" rather than silently dropped.
pub(crate) fn docker_run_to_compose(args: &Args) -> Result<String> {
    use serde_norway::{Mapping, Value};

    let raw = arg(
        args,
        tool("docker-run-to-docker-compose-converter"),
        "command",
    )
    .trim();
    if raw.is_empty() {
        bail!("no docker run command provided");
    }
    let mut tokens = shell_split(raw)?.into_iter().peekable();
    if tokens.peek().map(String::as_str) == Some("docker") {
        tokens.next();
    }
    match tokens.next().as_deref() {
        Some("run") | Some("create") => {}
        _ => bail!("expected a `docker run ...` command"),
    }

    // key -> yaml list key
    let list_flags: &[(&str, &str)] = &[
        ("-p", "ports"),
        ("--publish", "ports"),
        ("-v", "volumes"),
        ("--volume", "volumes"),
        ("-e", "environment"),
        ("--env", "environment"),
        ("--expose", "expose"),
        ("--cap-add", "cap_add"),
        ("--cap-drop", "cap_drop"),
        ("--device", "devices"),
        ("--dns", "dns"),
        ("--add-host", "extra_hosts"),
        ("-l", "labels"),
        ("--label", "labels"),
        ("--sysctl", "sysctls"),
        ("--tmpfs", "tmpfs"),
        ("--security-opt", "security_opt"),
        ("--network", "networks"),
        ("--net", "networks"),
        ("--link", "links"),
    ];
    let scalar_flags: &[(&str, &str)] = &[
        ("--restart", "restart"),
        ("-w", "working_dir"),
        ("--workdir", "working_dir"),
        ("-u", "user"),
        ("--user", "user"),
        ("--entrypoint", "entrypoint"),
        ("-h", "hostname"),
        ("--hostname", "hostname"),
        ("--ipc", "ipc"),
        ("--pid", "pid"),
        ("--platform", "platform"),
        ("--shm-size", "shm_size"),
        ("--stop-signal", "stop_signal"),
        ("--domainname", "domainname"),
        ("--mac-address", "mac_address"),
        ("-m", "mem_limit"),
        ("--memory", "mem_limit"),
    ];

    let mut lists: Vec<(&str, Vec<String>)> = Vec::new();
    let mut scalars: Vec<(&str, Value)> = Vec::new();
    let mut log_opts: Vec<(String, String)> = Vec::new();
    let mut log_driver: Option<String> = None;
    let mut name: Option<String> = None;
    let mut image: Option<String> = None;
    let mut command: Vec<String> = Vec::new();
    let mut untranslated: Vec<String> = Vec::new();

    let push_list = |lists: &mut Vec<(&str, Vec<String>)>, key: &'static str, v: String| match lists
        .iter_mut()
        .find(|(k, _)| *k == key)
    {
        Some((_, vals)) => vals.push(v),
        None => lists.push((key, vec![v])),
    };

    while let Some(tok) = tokens.next() {
        if image.is_some() {
            command.push(tok);
            continue;
        }
        if !tok.starts_with('-') {
            image = Some(tok);
            continue;
        }
        // --flag=value / -pvalue
        let (flag, inline) = match tok.split_once('=') {
            Some((f, v)) => (f.to_string(), Some(v.to_string())),
            None => (tok.clone(), None),
        };
        let mut take_value = |inline: Option<String>, flag: &str| -> Result<String> {
            match inline {
                Some(v) => Ok(v),
                None => tokens
                    .next()
                    .with_context(|| format!("{flag} expects a value")),
            }
        };

        if let Some((_, key)) = list_flags.iter().find(|(f, _)| *f == flag) {
            let v = take_value(inline, &flag)?;
            push_list(&mut lists, key, v);
            continue;
        }
        if let Some((_, key)) = scalar_flags.iter().find(|(f, _)| *f == flag) {
            let v = take_value(inline, &flag)?;
            scalars.push((key, Value::String(v)));
            continue;
        }
        match flag.as_str() {
            "--name" => name = Some(take_value(inline, "--name")?),
            "--log-driver" => log_driver = Some(take_value(inline, "--log-driver")?),
            "--log-opt" => {
                let v = take_value(inline, "--log-opt")?;
                let (k, val) = v.split_once('=').unwrap_or((v.as_str(), ""));
                log_opts.push((k.to_string(), val.to_string()));
            }
            "--privileged" => scalars.push(("privileged", Value::Bool(true))),
            "--read-only" => scalars.push(("read_only", Value::Bool(true))),
            "--init" => scalars.push(("init", Value::Bool(true))),
            "-d" | "--detach" => {} // compose has no equivalent, `up -d` does it
            "--rm" | "-a" | "--attach" | "--sig-proxy" | "--interactive-only" => {
                untranslated.push(flag)
            }
            "-i" | "--interactive" => scalars.push(("stdin_open", Value::Bool(true))),
            "-t" | "--tty" => scalars.push(("tty", Value::Bool(true))),
            "-it" | "-ti" => {
                scalars.push(("stdin_open", Value::Bool(true)));
                scalars.push(("tty", Value::Bool(true)));
            }
            // bare `-pVALUE` style short flags
            f if f.len() > 2 && !f.starts_with("--") => {
                let (short, v) = (&f[..2], f[2..].to_string());
                if let Some((_, key)) = list_flags.iter().find(|(fl, _)| *fl == short) {
                    push_list(&mut lists, key, v);
                } else if let Some((_, key)) = scalar_flags.iter().find(|(fl, _)| *fl == short) {
                    scalars.push((key, Value::String(v)));
                } else {
                    untranslated.push(f.to_string());
                }
            }
            f => untranslated.push(f.to_string()),
        }
    }

    let image = image.context("no image found in the docker run command")?;
    let service = name.clone().unwrap_or_else(|| {
        image
            .rsplit('/')
            .next()
            .unwrap_or(&image)
            .split(':')
            .next()
            .unwrap_or(&image)
            .to_string()
    });

    let mut svc = Mapping::new();
    svc.insert("image".into(), Value::String(image));
    if let Some(n) = name {
        svc.insert("container_name".into(), Value::String(n));
    }
    for (key, vals) in lists {
        svc.insert(
            key.into(),
            Value::Sequence(vals.into_iter().map(Value::String).collect()),
        );
    }
    for (key, v) in scalars {
        svc.insert(key.into(), v);
    }
    if log_driver.is_some() || !log_opts.is_empty() {
        let mut logging = Mapping::new();
        if let Some(d) = log_driver {
            logging.insert("driver".into(), Value::String(d));
        }
        if !log_opts.is_empty() {
            let mut opts = Mapping::new();
            for (k, v) in log_opts {
                opts.insert(k.into(), Value::String(v));
            }
            logging.insert("options".into(), Value::Mapping(opts));
        }
        svc.insert("logging".into(), Value::Mapping(logging));
    }
    if !command.is_empty() {
        svc.insert(
            "command".into(),
            Value::Sequence(command.into_iter().map(Value::String).collect()),
        );
    }

    let mut services = Mapping::new();
    services.insert(service.into(), Value::Mapping(svc));
    let mut root = Mapping::new();
    root.insert("services".into(), Value::Mapping(services));

    let mut out = serde_norway::to_string(&Value::Mapping(root)).context("could not emit YAML")?;
    if !untranslated.is_empty() {
        let _ = write!(
            out,
            "\n# not translated to docker-compose: {}\n",
            untranslated.join(", ")
        );
    }
    Ok(out)
}

pub static TOOL: Tool = Tool {
    id: "docker-run-to-docker-compose-converter",
    name: "Docker run to docker-compose converter",
    description: "Turn a docker run command into its docker-compose equivalent.",
    category: Category::Development,
    keywords: &[
        "docker", "run", "compose", "yaml", "yml", "convert", "deamon",
    ],
    fields: &[Field {
        key: "command",
        label: "Your docker run command",
        kind: crate::FieldKind::TextArea,
        default: "docker run -p 80:80 -v /var/run/docker.sock:/tmp/docker.sock:ro --restart always --log-opt max-size=1g nginx",
        options: &[],
    }],
    kind: Kind::Pure,
    run: docker_run_to_compose,
};

#[cfg(test)]
mod tests {
    use super::*;
    use crate::testing::*;

    #[test]
    fn docker_run_conversion() {
        let out = run("docker-run-to-docker-compose-converter", &[(
            "command",
            "docker run -p 80:80 -v /var/run/docker.sock:/tmp/docker.sock:ro --restart always --log-opt max-size=1g nginx",
        )])
        .unwrap();
        assert!(out.contains("services:"), "{out}");
        assert!(out.contains("nginx:"), "{out}");
        assert!(out.contains("image: nginx"), "{out}");
        assert!(out.contains("80:80"), "{out}");
        assert!(
            out.contains("/var/run/docker.sock:/tmp/docker.sock:ro"),
            "{out}"
        );
        assert!(out.contains("restart: always"), "{out}");
        // serde_norway emits plain scalars where the shim quoted them
        // (`max-size: 1g`, `image: postgres:16`); both are valid YAML.
        assert!(out.contains("max-size: 1g"), "{out}");

        let out = run("docker-run-to-docker-compose-converter", &[(
            "command",
            "docker run -d --name db -e POSTGRES_PASSWORD=secret -p 5432:5432 postgres:16 postgres -c log_statement=all",
        )])
        .unwrap();
        assert!(out.contains("db:"), "{out}");
        assert!(out.contains("container_name: db"), "{out}");
        assert!(out.contains("POSTGRES_PASSWORD=secret"), "{out}");
        assert!(out.contains("image: postgres:16"), "{out}");
        assert!(out.contains("command:"), "{out}");

        assert!(
            run(
                "docker-run-to-docker-compose-converter",
                &[("command", "docker run")]
            )
            .is_err()
        );
        assert!(
            run(
                "docker-run-to-docker-compose-converter",
                &[("command", "kubectl get pods")]
            )
            .is_err()
        );
        assert!(
            run(
                "docker-run-to-docker-compose-converter",
                &[("command", "docker run -e 'unclosed nginx")]
            )
            .is_err()
        );
    }

    #[test]
    fn shell_split_handles_quotes() {
        assert_eq!(shell_split("a  b\tc").unwrap(), ["a", "b", "c"]);
        assert_eq!(shell_split("-e 'A=b c'").unwrap(), ["-e", "A=b c"]);
        assert_eq!(shell_split("-e \"A=b c\"").unwrap(), ["-e", "A=b c"]);
        assert_eq!(shell_split("''").unwrap(), [""]);
        assert!(shell_split("'oops").is_err());
    }
}
