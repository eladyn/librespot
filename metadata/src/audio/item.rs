use std::fmt::Debug;

use crate::{
    availability::{AudioItemAvailability, Availabilities, UnavailabilityReason},
    episode::Episode,
    error::MetadataError,
    restriction::Restrictions,
    track::Track,
    Metadata,
};

use super::file::AudioFiles;

use librespot_core::{
    date::Date, session::UserData, spotify_id::SpotifyItemType, Error, Session, SpotifyId,
};

pub type AudioItemResult = Result<AudioItem, Error>;

#[derive(Debug, Clone)]
pub enum AudioItem {
    Track(Track),
    Episode(Episode),
}

impl AudioItem {
    pub async fn get_file(session: &Session, id: SpotifyId) -> AudioItemResult {
        Ok(match id.item_type {
            SpotifyItemType::Track => AudioItem::Track(Track::get(session, &id).await?),
            SpotifyItemType::Episode => AudioItem::Episode(Episode::get(session, &id).await?),
            _ => return Err(Error::unavailable(MetadataError::NonPlayable)),
        })
    }

    pub fn id(&self) -> SpotifyId {
        match self {
            AudioItem::Track(t) => t.id,
            AudioItem::Episode(e) => e.id,
        }
    }

    pub fn spotify_uri(&self) -> Result<String, Error> {
        self.id().to_uri()
    }

    pub fn name(&self) -> &str {
        match self {
            AudioItem::Track(t) => &t.name,
            AudioItem::Episode(e) => &e.name,
        }
    }

    pub fn duration(&self) -> i32 {
        match self {
            AudioItem::Track(t) => t.duration,
            AudioItem::Episode(e) => e.duration,
        }
    }

    pub fn is_explicit(&self) -> bool {
        match self {
            AudioItem::Track(t) => t.is_explicit,
            AudioItem::Episode(e) => e.is_explicit,
        }
    }

    pub fn files(&self) -> &AudioFiles {
        match self {
            AudioItem::Track(t) => &t.files,
            AudioItem::Episode(e) => &e.audio,
        }
    }

    pub fn availability(&self, session: &Session) -> AudioItemAvailability {
        let (availability, restrictions) = match self {
            AudioItem::Track(t) => {
                if Date::now_utc() < t.earliest_live_timestamp {
                    return Err(UnavailabilityReason::Embargo);
                }
                (&t.availability, &t.restrictions)
            }
            AudioItem::Episode(e) => (&e.availability, &e.restrictions),
        };

        available_for_user(&session.user_data(), availability, restrictions)
    }
}

fn allowed_for_user(user_data: &UserData, restrictions: &Restrictions) -> AudioItemAvailability {
    let country = &user_data.country;
    let user_catalogue = match user_data.attributes.get("catalogue") {
        Some(catalogue) => catalogue,
        None => "premium",
    };

    for premium_restriction in restrictions.iter().filter(|restriction| {
        restriction
            .catalogue_strs
            .iter()
            .any(|restricted_catalogue| restricted_catalogue == user_catalogue)
    }) {
        if let Some(allowed_countries) = &premium_restriction.countries_allowed {
            // A restriction will specify either a whitelast *or* a blacklist,
            // but not both. So restrict availability if there is a whitelist
            // and the country isn't on it.
            if allowed_countries.iter().any(|allowed| country == allowed) {
                return Ok(());
            } else {
                return Err(UnavailabilityReason::NotWhitelisted);
            }
        }

        if let Some(forbidden_countries) = &premium_restriction.countries_forbidden {
            if forbidden_countries
                .iter()
                .any(|forbidden| country == forbidden)
            {
                return Err(UnavailabilityReason::Blacklisted);
            } else {
                return Ok(());
            }
        }
    }

    Ok(()) // no restrictions in place
}

fn available(availability: &Availabilities) -> AudioItemAvailability {
    if availability.is_empty() {
        // not all items have availability specified
        return Ok(());
    }

    if !(availability
        .iter()
        .any(|availability| Date::now_utc() >= availability.start))
    {
        return Err(UnavailabilityReason::Embargo);
    }

    Ok(())
}

fn available_for_user(
    user_data: &UserData,
    availability: &Availabilities,
    restrictions: &Restrictions,
) -> AudioItemAvailability {
    available(availability)?;
    allowed_for_user(user_data, restrictions)?;
    Ok(())
}
