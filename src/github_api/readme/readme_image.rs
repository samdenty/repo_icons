use super::{primary_heading::PrimaryHeading, Readme};
use crate::blacklist::is_badge;
use gh_api::get_token;
use scraper::ElementRef;
use serde::{Deserialize, Serialize};
use std::{
  cmp::Ordering,
  collections::{HashMap, HashSet},
};
use url::Url;

#[derive(Debug, Serialize, Deserialize, PartialEq, Eq)]
pub enum ProjectLink {
  Website,
  Repo,
}

#[derive(Debug, Serialize, Deserialize, PartialEq, Eq, Hash)]
#[serde(rename_all = "snake_case")]
pub enum KeywordMention {
  Logo,
  Banner,
  RepoName,
}

#[derive(Debug, Deserialize, Serialize, PartialEq, Eq)]
pub struct ReadmeImage {
  pub src: Url,
  pub headers: HashMap<String, String>,
  /// whether the image was in the primary markdown heading
  pub in_primary_heading: bool,
  /// whether the image was the first/last one in the heading
  pub edge_of_primary_heading: bool,
  /// whether the image mentions a keyword in its src / alt text
  pub keyword_mentions: HashSet<KeywordMention>,
  /// whether the image src points to a file inside of the repo
  pub sourced_from_repo: bool,
  /// whether the image has links to the projects
  pub links_to: Option<ProjectLink>,
  /// whether the image has the CSS "align: center"
  pub is_align_center: bool,
  /// whether the image has height or width attributes
  pub has_size_attrs: bool,
}

impl ReadmeImage {
  pub async fn get(
    readme: &Readme,
    elem_ref: &ElementRef<'_>,
    primary_heading: &mut PrimaryHeading<'_>,
  ) -> Option<Self> {
    let elem = elem_ref.value();

    let src = elem
      .attr("data-canonical-src")
      .or(elem.attr("src"))
      .and_then(|src| readme.qualify_url(src).ok())
      .unwrap();

    if is_badge(&src) {
      return None;
    }

    let cdn_src = elem
      .attr("data-canonical-src")
      .and(elem.attr("src"))
      .and_then(|src| readme.qualify_url(src).ok());

    let mut is_align_center = false;
    let mut links_to = None;
    for elem_ref in elem_ref.ancestors().map(ElementRef::wrap).flatten() {
      let element = elem_ref.value();

      if element.attr("align") == Some("center") {
        is_align_center = true;
      }

      if element.name() == "a" && links_to.is_none() {
        links_to = match element
          .attr("href")
          .and_then(|href| readme.qualify_url(href).ok())
        {
          Some(href) => {
            // if the img points to the same url as the link
            // then its a default url generated by github
            let mut img_blob_url = src.clone();
            img_blob_url.set_path(&src.path().replacen("/raw/", "/blob/", 1));
            if href != img_blob_url {
              readme.is_link_to_project(&href).await
            } else {
              None
            }
          }
          None => None,
        };
      }
    }

    let branch_and_path = readme.get_branch_and_path(&src).await;
    let keyword_mentions = {
      let mut mentions = HashSet::new();

      let mut path = &src.path().to_lowercase();
      if let Some((_, file_path)) = &branch_and_path {
        path = file_path;
      }

      let alt = elem
        .attr("alt")
        .map(|alt| alt.to_lowercase())
        .unwrap_or(String::new());

      if path.contains("logo") || alt.contains("logo") {
        mentions.insert(KeywordMention::Logo);
      }

      if path.contains("banner") || alt.contains("banner") {
        mentions.insert(KeywordMention::Banner);
      }

      if path.contains(&readme.repo) || alt.contains(&readme.repo) {
        mentions.insert(KeywordMention::RepoName);
      };
      mentions
    };

    let mut headers = HashMap::new();

    let src = cdn_src.unwrap_or({
      if let Some((branch, path)) = &branch_and_path {
        if readme.private {
          headers.insert(
            "Authorization".to_string(),
            format!("Bearer {}", get_token().unwrap()).to_string(),
          );
        }

        Url::parse(&format!(
          "https://raw.githubusercontent.com/{}/{}/{}/{}",
          readme.owner, readme.repo, branch, path
        ))
        .unwrap()
      } else {
        src
      }
    });

    Some(ReadmeImage {
      src,
      headers,
      in_primary_heading: primary_heading.contains(elem_ref),
      edge_of_primary_heading: false,
      keyword_mentions,
      sourced_from_repo: branch_and_path.is_some(),
      links_to,
      is_align_center,
      has_size_attrs: elem.attr("width").or(elem.attr("height")).is_some(),
    })
  }

  pub fn weight(&self) -> u8 {
    let mut weight = 0;

    if self.in_primary_heading {
      weight += 2;

      if self.is_align_center {
        weight += 2;
      }

      if self.has_size_attrs {
        weight += 2;
      }

      if self.sourced_from_repo {
        weight += 4;
      }
    };

    if self.edge_of_primary_heading {
      weight += 4;
    }

    match self.links_to {
      Some(ProjectLink::Website) => {
        weight += 8;
      }
      Some(ProjectLink::Repo) => {
        weight += 4;
      }
      None => {}
    }

    if self.keyword_mentions.contains(&KeywordMention::Logo) {
      weight += 16
    }

    if self.keyword_mentions.contains(&KeywordMention::Banner) {
      weight += 8
    }

    if self.keyword_mentions.contains(&KeywordMention::RepoName) {
      weight += 4
    }

    weight
  }
}

impl Ord for ReadmeImage {
  fn cmp(&self, other: &Self) -> Ordering {
    other.weight().cmp(&self.weight())
  }
}

impl PartialOrd for ReadmeImage {
  fn partial_cmp(&self, other: &Self) -> Option<Ordering> {
    Some(self.cmp(other))
  }
}