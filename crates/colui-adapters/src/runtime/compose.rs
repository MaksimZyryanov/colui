use colui_domain::ProjectProfile;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ComposeOperation {
    Up,
    Stop,
    Down,
    Restart,
}

pub fn compose_args(profile: &ProjectProfile, operation: ComposeOperation) -> Vec<String> {
    let mut args = vec!["compose".into()];
    for path in &profile.compose_files {
        args.extend(["-f".into(), path.to_string_lossy().into_owned()]);
    }
    let project_name = serde_json::to_string(&profile.compose_project_name)
        .expect("validated Compose project name is serializable")
        .trim_matches('"')
        .to_owned();
    args.extend(["--project-name".into(), project_name]);
    for path in &profile.environment_files {
        args.extend(["--env-file".into(), path.to_string_lossy().into_owned()]);
    }
    match operation {
        ComposeOperation::Up => args.extend(["up".into(), "-d".into()]),
        ComposeOperation::Stop => args.push("stop".into()),
        ComposeOperation::Down => args.push("down".into()),
        ComposeOperation::Restart => args.push("restart".into()),
    }
    args
}
