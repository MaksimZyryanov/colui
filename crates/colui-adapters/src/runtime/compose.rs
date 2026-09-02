use colui_domain::ProjectProfile;

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum ComposeOperation {
    Up,
    Stop,
    Down,
    Restart,
    UpScaled { service: String, replicas: u16 },
}

pub fn compose_args(profile: &ProjectProfile, operation: ComposeOperation) -> Vec<String> {
    let mut args = vec!["compose".into()];
    for path in &profile.compose_files {
        args.extend(["-f".into(), resolve_path(&profile.working_directory, path)]);
    }
    let project_name = serde_json::to_string(&profile.compose_project_name)
        .expect("validated Compose project name is serializable")
        .trim_matches('"')
        .to_owned();
    args.extend(["--project-name".into(), project_name]);
    for path in &profile.environment_files {
        args.extend([
            "--env-file".into(),
            resolve_path(&profile.working_directory, path),
        ]);
    }
    match operation {
        ComposeOperation::Up => args.extend(["up".into(), "-d".into()]),
        ComposeOperation::UpScaled { service, replicas } => args.extend([
            "up".into(),
            "-d".into(),
            "--scale".into(),
            format!("{service}={replicas}"),
        ]),
        ComposeOperation::Stop => args.push("stop".into()),
        ComposeOperation::Down => args.push("down".into()),
        ComposeOperation::Restart => args.push("restart".into()),
    }
    args
}

fn resolve_path(working_directory: &std::path::Path, path: &std::path::Path) -> String {
    if path.is_absolute() {
        path.to_string_lossy().into_owned()
    } else {
        working_directory.join(path).to_string_lossy().into_owned()
    }
}
