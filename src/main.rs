use crate::{
    bitbucket_client::BitbucketClient,
    models::{PackageReference, RepoFile, RepoPackageReference},
    report::PackageVersionReport,
};
use clap::{Parser, ValueEnum};
mod bitbucket_client;
mod models;
mod report;

#[derive(Copy, Clone, PartialEq, Eq, PartialOrd, Ord, ValueEnum, Debug)]
enum OutputKind {
    Console,
    Txt,
    Md,
}

#[derive(Parser, Debug)]
#[command(author, version, about, long_about = None)]
struct Args {
    /// The url of the Bitbucket instance. Should be specified without protocol.
    #[arg(long)]
    base_url: String,
    /// The key/name of the project you want to fetch repos from.
    #[arg(long)]
    project: String,
    /// The name of the package you want to check the version of. This must match the name as specified in the .csproj files exactly.
    #[arg(long)]
    package: String,
    /// HTTP access token for Bitbucket.
    #[arg(long)]
    token: String,
    /// [Optional] Specifies whether to print the output to the console, or save it to a .txt or .md file.
    #[arg(long, value_enum, default_value_t=OutputKind::Console)]
    output_kind: OutputKind,
    /// [Optional] If you have repos you wish to ignore that share a common prefix, you can exclude them by specifying the prefix here.
    #[arg(long)]
    ignore_repo_prefix: Option<String>,
    /// [Optional] Specifies the name of the file to save the output to. Ignored if 'output-kind' is 'console'
    #[arg(long)]
    output_file_name: Option<String>,
}

#[tokio::main]
async fn main() -> Result<(), anyhow::Error> {
    let args = Args::parse();

    let mut report = PackageVersionReport::default();

    let token = args.token;
    let package = args.package;
    let project = args.project;
    let ignore_repo_prefix = args.ignore_repo_prefix;
    let output_kind = args.output_kind;
    let output_file_name = args
        .output_file_name
        .unwrap_or("package-version-report".to_string());

    let client = BitbucketClient::new(&args.base_url, &token);

    let project_repos = client.get_project_repos(&project).await?;

    for repo in project_repos.iter() {
        if let Some(ref ignore_prefix) = ignore_repo_prefix {
            if repo.slug.starts_with(&ignore_prefix.clone()) {
                println!("Ignored repo: {}", &repo.slug);
                continue;
            }
        }

        let csproj_files = client
            .get_paths_of_files_in_repo(&project, &repo.slug, Some(".csproj"))
            .await?;

        let get_file_content_futures = csproj_files.iter().map(|f| async {
            client
                .get_repo_file_content(&project, &repo.slug, f)
                .await
                .map(|res| RepoFile {
                    name: f.clone(),
                    content: res,
                })
        });

        let csproj_files_with_content: Vec<RepoFile> =
            futures::future::join_all(get_file_content_futures)
                .await
                .into_iter()
                .filter_map(|f| f.ok())
                .collect();

        let mut repo_package_references: Vec<RepoPackageReference> = Vec::new();

        for file in csproj_files_with_content.into_iter() {
            let file_references: Vec<RepoPackageReference> = file
                .content
                .lines
                .iter()
                .filter_map(|line| PackageReference::try_from(line.clone()).ok())
                .filter(|package_reference| package_reference.package_name == package)
                .map(|package_reference| {
                    RepoPackageReference::new(
                        repo.slug.to_string(),
                        file.name.clone(),
                        package_reference,
                    )
                })
                .collect();

            repo_package_references.extend_from_slice(&file_references);
        }

        report
            .repo_package_references
            .extend_from_slice(&repo_package_references);

        println!("Done processing files from repo: {}", &repo.slug);
    }

    match output_kind {
        OutputKind::Console => println!("{}", report.to_string()),
        OutputKind::Txt => {
            std::fs::write(format!("{output_file_name}.txt"), report.to_string())?;
        }
        OutputKind::Md => {
            std::fs::write(format!("{output_file_name}.md"), report.to_string())?;
        }
    }

    Ok(())
}
