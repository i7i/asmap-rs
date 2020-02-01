pub(crate) use crate::common::*;

pub(crate) fn parse_mrt_from_gz_url(
    url: &str,
    mrt_hm: &mut HashMap<Address, HashSet<Vec<u32>>>,
) -> Result<(), Error> {
    let mut addresses: Vec<Address> = Vec::new();

    let res = reqwest::blocking::get(url).map_err(|reqwest_error| Error::Reqwest {
        url: url.to_string(),
        reqwest_error,
    })?;

    let decoder = GzDecoder::new(res);

    let mut reader = Reader { stream: decoder };

    while let Ok(Some((_, record))) = reader.read() {
        match record {
            Record::TABLE_DUMP_V2(tdv2_entry) => match tdv2_entry {
                TABLE_DUMP_V2::PEER_INDEX_TABLE(entry) => {
                    for peer_entry in entry.peer_entries {
                        let addr = Address {
                            ip: peer_entry.peer_ip_address,
                            mask: None,
                        };
                        addresses.push(addr);
                    }
                }
                TABLE_DUMP_V2::RIB_IPV4_UNICAST(entry) => {
                    let mask = entry.prefix_length;
                    for rib_entry in entry.entries {
                        let index = rib_entry.peer_index as usize;
                        addresses[index].mask = Some(entry.prefix_length);

                        let mut as_path = as_path_from_bgp_attributes(rib_entry.attributes)?;
                        as_path.dedup();

                        mrt_hm
                            .entry(addresses[index])
                            .or_insert_with(HashSet::new)
                            .insert(as_path);
                    }
                }
                _ => continue,
            },
            _ => continue,
        }
    }

    Ok(())
}

pub(crate) fn parse_mrt_from_file(
    path: &str,
    mrt_hm: &mut HashMap<Address, HashSet<Vec<u32>>>,
) -> Result<(), Error> {
    let mut addresses: Vec<Address> = Vec::new();

    let mut buffer =
        BufReader::new(File::open(path).map_err(|error| Error::IoError { io_error: error })?);

    let mut reader = Reader { stream: buffer };

    while let Ok(Some((_, record))) = reader.read() {
        match record {
            Record::TABLE_DUMP_V2(tdv2_entry) => match tdv2_entry {
                TABLE_DUMP_V2::PEER_INDEX_TABLE(entry) => {
                    for peer_entry in entry.peer_entries {
                        let addr = Address {
                            ip: peer_entry.peer_ip_address,
                            mask: None,
                        };
                        addresses.push(addr);
                    }
                }
                TABLE_DUMP_V2::RIB_IPV4_UNICAST(entry) => {
                    let mask = entry.prefix_length;
                    for rib_entry in entry.entries {
                        let index = rib_entry.peer_index as usize;
                        addresses[index].mask = Some(entry.prefix_length);

                        let mut as_path = as_path_from_bgp_attributes(rib_entry.attributes)?;
                        as_path.dedup();

                        mrt_hm
                            .entry(addresses[index])
                            .or_insert_with(HashSet::new)
                            .insert(as_path);
                    }
                }
                _ => continue,
            },
            _ => continue,
        }
    }

    Ok(())
}

fn as_path_from_bgp_attributes(bgp_attributes: Vec<u8>) -> Result<Vec<u32>, Error> {
    // Ok(vec![1234, 5678])
    todo!();
}

pub(crate) fn find_as_bottleneck(
    mrt_hm: &mut HashMap<Address, HashSet<Vec<u32>>>,
) -> Result<HashMap<Address, u32>, Error> {
    let mut prefix_to_common_suffix: HashMap<Address, Vec<u32>> = HashMap::new();

    find_common_suffix(mrt_hm, &mut prefix_to_common_suffix)?;

    let mut as_bottleneck: HashMap<Address, u32> = HashMap::new();
    for (addr, mut as_path) in prefix_to_common_suffix {
        let asn = match as_path.pop() {
            Some(a) => a,
            None => panic!("ahhh! no asn :("),
        };
        as_bottleneck.insert(addr, asn);
    }

    Ok(as_bottleneck)
}

fn find_common_suffix(
    mrt_hm: &mut HashMap<Address, HashSet<Vec<u32>>>,
    prefix_to_common_suffix: &mut HashMap<Address, Vec<u32>>,
) -> Result<(), Error> {
    for (prefix, as_paths) in mrt_hm.iter() {
        let mut as_paths_sorted: Vec<&Vec<u32>> = as_paths.iter().collect();

        as_paths_sorted.sort_by(|a, b| a.len().cmp(&b.len())); // descending

        let mut rev_common_suffix: Vec<u32> = as_paths_sorted[0].to_vec();
        // rev_common_suffix.reverse();
        for as_path in as_paths_sorted.iter().skip(1) {
            // first one is already in rev_common_suffix
            let mut rev_as_path: Vec<u32> = as_path.to_vec();
            // rev_as_path.reverse();

            // every IP should always belong to only one AS
            assert!(rev_common_suffix.first() == rev_as_path.first());

            // first element is already checked
            for i in 1..rev_common_suffix.len() {
                if rev_as_path[i] != rev_common_suffix[i] {
                    rev_common_suffix.truncate(i);
                    break;
                }
            }
        }
        // rev_common_suffix.reverse();
        prefix_to_common_suffix
            .entry(*prefix)
            .or_insert(rev_common_suffix);
    }

    Ok(())
}

pub(crate) fn write_bottleneck(mrt_hm: HashMap<Address, u32>) -> Result<(), Error> {
    todo!();
}

#[cfg(test)]
mod tests {
    use super::*;

    fn setup_mrt_hm() -> Result<HashMap<Address, HashSet<Vec<u32>>>, Error> {
        let mut mrt_hm: HashMap<Address, HashSet<Vec<u32>>> = HashMap::new();

        mrt_hm
            .entry(Address::from_str("195.66.225.77/0")?)
            .or_insert_with(HashSet::new)
            .insert(vec![64271, 62240, 3356]);

        mrt_hm
            .entry(Address::from_str("195.66.225.77/0")?)
            .or_insert_with(HashSet::new)
            .insert(vec![64271, 62240, 174]);

        mrt_hm
            .entry(Address::from_str("5.57.81.186/24")?)
            .or_insert_with(HashSet::new)
            .insert(vec![6894, 13335, 38803, 56203]);

        mrt_hm
            .entry(Address::from_str("5.57.81.186/24")?)
            .or_insert_with(HashSet::new)
            .insert(vec![6894, 13335, 4826, 174]);

        Ok(mrt_hm)
    }

    #[test]
    fn finds_common_suffix_from_mrt_hashmap() -> Result<(), Error> {
        let mut want: HashMap<Address, Vec<u32>> = HashMap::new();
        want.insert(Address::from_str("195.66.225.77/0")?, vec![64271, 62240]);
        want.insert(Address::from_str("5.57.81.186/24")?, vec![6894, 13335]);

        let mut mrt_hm = setup_mrt_hm()?;
        let mut have: HashMap<Address, Vec<u32>> = HashMap::new();

        assert_eq!(find_common_suffix(&mut mrt_hm, &mut have)?, ());
        assert_eq!(have, want);

        Ok(())
    }

    #[test]
    fn finds_as_bottleneck_from_mrt_hashmap() -> Result<(), Error> {
        let mut want: HashMap<Address, u32> = HashMap::new();
        want.insert(Address::from_str("195.66.225.77/0")?, 62240);
        want.insert(Address::from_str("5.57.81.186/24")?, 13335);

        let mut mrt_hm = setup_mrt_hm()?;
        let have = find_as_bottleneck(&mut mrt_hm)?;

        assert_eq!(have, want);

        Ok(())
    }

    #[ignore]
    #[test]
    fn can_parse_mrt_from_file() -> Result<(), Error> {
        let mut mrt_hm: HashMap<Address, HashSet<Vec<u32>>> = HashMap::new();
        let path = "data/latest-bview-2020-01-28-160000";
        assert_eq!(parse_mrt_from_file(path, &mut mrt_hm)?, ());
        assert_eq!(mrt_hm.is_empty(), false);
        Ok(())
    }

    #[ignore]
    #[test]
    fn can_parse_mrt_from_gz_url() -> Result<(), Error> {
        let mut mrt_hm: HashMap<Address, HashSet<Vec<u32>>> = HashMap::new();
        let url = "http://data.ris.ripe.net/rrc01/latest-bview.gz";
        assert_eq!(parse_mrt_from_gz_url(url, &mut mrt_hm)?, ());
        assert_eq!(mrt_hm.is_empty(), false);
        Ok(())
    }
}