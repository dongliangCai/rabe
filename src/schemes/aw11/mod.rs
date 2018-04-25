//! This is the documentation for the `AW11` scheme.
//!
//! * Developped by Lewko, Allison, and Brent Waters, "Decentralizing Attribute-Based Encryption.", see Appendix D
//! * Published in Eurocrypt 2011
//! * Available from http://eprint.iacr.org/2010/351.pdf
//! * Type:			encryption (identity-based)
//! * Setting:		bilinear groups (asymmetric)
//! * Authors:		Georg Bramm
//! * Date:			04/2018
//!
//! # Examples
//!
//! ```
//!use rabe::schemes::aw11::*;
//!let gk = setup();
//!let (pk, msk) = authgen(&gk, &vec!["A".to_string(), "B".to_string()]).unwrap();
//!let plaintext = String::from("our plaintext!").into_bytes();
//!let policy = String::from(r#"{"OR": [{"ATT": "A"}, {"ATT": "B"}]}"#);
//!let bob = keygen(&gk, &msk, &String::from("bob"), &vec!["A".to_string()]).unwrap();
//!let ct: Aw11Ciphertext = encrypt(&gk, &vec![pk], &policy, &plaintext).unwrap();
//!let matching = decrypt(&gk, &bob, &ct).unwrap();
//!assert_eq!(matching, plaintext);
//! ```
extern crate bn;
extern crate rand;
extern crate serde;
extern crate serde_json;

use std::string::String;
use bn::*;
use utils::policy::msp::AbePolicy;
use utils::secretsharing::{gen_shares_str, calc_coefficients_str, calc_pruned_str};
use utils::tools::*;
use utils::aes::*;
use utils::hash::blake2b_hash_g1;

/// An AW11 Global Parameters Key (GK)
#[derive(Serialize, Deserialize, PartialEq, Clone)]
pub struct Aw11GlobalKey {
    pub _g1: bn::G1,
    pub _g2: bn::G2,
}

/// An AW11 Public Key (PK)
#[derive(Serialize, Deserialize, PartialEq, Clone)]
pub struct Aw11PublicKey {
    pub _attr: Vec<(String, bn::Gt, bn::G2)>,
}

/// An AW11 Master Key (MK)
#[derive(Serialize, Deserialize, PartialEq, Clone)]
pub struct Aw11MasterKey {
    pub _attr: Vec<(String, bn::Fr, bn::Fr)>,
}

/// An AW11 Ciphertext (CT)
#[derive(Serialize, Deserialize, PartialEq, Clone)]
pub struct Aw11Ciphertext {
    pub _policy: String,
    pub _c_0: bn::Gt,
    pub _c: Vec<(String, bn::Gt, bn::G2, bn::G2)>,
    pub _ct: Vec<u8>,
}

/// An AW11 Secret Key (SK)
#[derive(Serialize, Deserialize, PartialEq, Clone)]
pub struct Aw11SecretKey {
    pub _gid: String,
    pub _attr: Vec<(String, bn::G1)>,
}

/// A global Context for an AW11 Global Parameters Key (GP)
#[derive(Serialize, Deserialize, PartialEq, Clone)]
pub struct Aw11GlobalContext {
    pub _gk: Aw11GlobalKey,
}

/// A Context for an AW11 Key Pair (MK/PK)
#[derive(Serialize, Deserialize, PartialEq, Clone)]
pub struct Aw11Context {
    pub _msk: Aw11MasterKey,
    pub _pk: Aw11PublicKey,
}

/// Sets up a new AW11 Scheme by creating a Global Parameters Key (GK)
pub fn setup() -> Aw11GlobalKey {
    // random number generator
    let _rng = &mut rand::thread_rng();
    // generator of group G1: g1 and generator of group G2: g2
    let _gk = Aw11GlobalKey {
        _g1: G1::random(_rng),
        _g2: G2::random(_rng),
    };
    // return PK and MSK
    return _gk;
}

/// Sets up a new AW11 Authority by creating a key pair: Public Parameters Key (PK) and Master Key (MK)
///
/// # Arguments
///
///	* `_gk` - A Global Parameters Key (GK), generated by the function setup()
///	* `_attributes` - A Vector of String attributes assigned to this Authority
///
/// # Remarks
///
/// In this scheme, all attributes are converted to upper case bevor calculation, i.e. they are case insensitive
/// This means that all attributes "tEsT", "TEST" and "test" are the same in this scheme.
pub fn authgen(
    _gk: &Aw11GlobalKey,
    _attributes: &Vec<String>,
) -> Option<(Aw11PublicKey, Aw11MasterKey)> {
    // if no attibutes or an empty policy
    // maybe add empty msk also here
    if _attributes.is_empty() {
        return None;
    }
    // random number generator
    let _rng = &mut rand::thread_rng();
    // generator of group G1: g and generator of group G2: h
    let mut _sk: Vec<(String, bn::Fr, bn::Fr)> = Vec::new(); //dictionary of {s: {alpha_i, y_i}}
    let mut _pk: Vec<(String, bn::Gt, bn::G2)> = Vec::new(); // dictionary of {s: {e(g,g)^alpha_i, g1^y_i}}
    // now calculate attribute values
    for _attr in _attributes {
        // calculate randomness
        let _alpha_i = Fr::random(_rng);
        let _y_i = Fr::random(_rng);
        _sk.push((_attr.clone().to_uppercase(), _alpha_i, _y_i));
        _pk.push((
            _attr.clone().to_uppercase(),
            pairing(_gk._g1, _gk._g2).pow(_alpha_i),
            _gk._g2 * _y_i,
        ));
    }
    // return PK and MSK
    return Some((Aw11PublicKey { _attr: _pk }, Aw11MasterKey { _attr: _sk }));
}

/// Sets up and generates a new User by creating a secret user key (SK). The key is created for a user with a given "name" on the given set of attributes.
///
/// # Arguments
///
///	* `_gk` - A Global Parameters Key (GK), generated by setup()
///	* `_msk` - A Master Key (MK), associated with an authority and generated by authgen()
///	* `_name` - The name of the user the key is associated with. Must be unique.
///	* `_attributes` - A Vector of String attributes assigned to this User
///
/// # Remarks
///
/// In this scheme, all attributes are converted to upper case bevor calculation, i.e. they are case insensitive
/// This means that all attributes "tEsT", "TEST" and "test" are treated the same in this scheme.
pub fn keygen(
    _gk: &Aw11GlobalKey,
    _msk: &Aw11MasterKey,
    _name: &String,
    _attributes: &Vec<String>,
) -> Option<Aw11SecretKey> {
    // if no attibutes or no gid
    if _attributes.is_empty() || _name.is_empty() {
        return None;
    }
    let mut _sk: Aw11SecretKey = Aw11SecretKey {
        _gid: _name.clone(),
        _attr: Vec::new(),
    };
    for _attribute in _attributes {
        add_attribute(_gk, _msk, _attribute, &mut _sk);
    }
    return Some(_sk);
}

/// This function does not create a new User key, but adds a new attribute to an already generated key (SK).
///
/// # Arguments
///
///	* `_gk` - A Global Parameters Key (GK), generated by setup()
///	* `_msk` - A Master Key (MK), associated with an authority and generated by authgen()
///	* `_attribute` - A String attribute that should be added to the already existing key (SK)
///	* `_sk` - The secret user key (SK)
pub fn add_attribute(
    _gk: &Aw11GlobalKey,
    _msk: &Aw11MasterKey,
    _attribute: &String,
    _sk: &mut Aw11SecretKey,
) {
    // if no attibutes or no gid
    if _attribute.is_empty() || _sk._gid.is_empty() {
        return;
    }
    let _h_g1 = blake2b_hash_g1(_gk._g1, &_sk._gid);
    let _auth_attribute = _msk._attr
        .iter()
        .filter(|_attr| _attr.0 == _attribute.to_string())
        .nth(0)
        .unwrap();
    _sk._attr.push((
        _auth_attribute.0.clone().to_uppercase(),
        (_gk._g1 * _auth_attribute.1) + (_h_g1 * _auth_attribute.2),
    ));
}

/// This function encrypts plaintext data using a given JSON String policy and produces a 'Aw11Ciphertext' if successfull.
///
/// # Arguments
///
///	* `_gk` - A Global Parameters Key (GK), generated by setup()
///	* `_pk` - A Public Parameters Key (MK), associated with an authority and generated by authgen()
///	* `_policy` - A JSON String policy describing the access rights
///	* `_plaintext` - The plaintext data given as a Vector of u8.
pub fn encrypt(
    _gk: &Aw11GlobalKey,
    _pks: &Vec<Aw11PublicKey>,
    _policy: &String,
    _plaintext: &[u8],
) -> Option<Aw11Ciphertext> {
    // random number generator
    let _rng = &mut rand::thread_rng();
    // an msp policy from the given String
    let msp: AbePolicy = AbePolicy::from_string(&_policy).unwrap();
    let _num_cols = msp._m[0].len();
    let _num_rows = msp._m.len();
    // pick randomness
    let _s = Fr::random(_rng);
    // and calculate shares "s" and "zero"
    let _s_shares = gen_shares_str(_s, _policy).unwrap();
    let _w_shares = gen_shares_str(Fr::zero(), _policy).unwrap();
    // calculate c0 with a randomly selected "msg"
    let _msg = pairing(G1::random(_rng), G2::random(_rng));
    let _c_0 = _msg * pairing(_gk._g1, _gk._g2).pow(_s);
    // now calculate the C1,x C2,x and C3,x parts
    let mut _c: Vec<(String, bn::Gt, bn::G2, bn::G2)> = Vec::new();
    for (_i, (_attr_name, _attr_share)) in _s_shares.into_iter().enumerate() {
        let _r_x = Fr::random(_rng);
        let _pk_attr = find_pk_attr(_pks, &_attr_name.to_uppercase()).unwrap();
        _c.push((
            _attr_name.clone().to_uppercase(),
            pairing(_gk._g1, _gk._g2).pow(_attr_share) *
                _pk_attr.1.pow(_r_x),
            _gk._g2 * _r_x,
            (_pk_attr.2 * _r_x) + (_gk._g2 * _w_shares[_i].1),
        ));
    }
    //println!("enc: {:?}", serde_json::to_string(&_msg).unwrap());
    //Encrypt plaintext using derived key from secret
    return Some(Aw11Ciphertext {
        _policy: _policy.clone(),
        _c_0: _c_0,
        _c: _c,
        _ct: encrypt_symmetric(&_msg, &_plaintext.to_vec()).unwrap(),
    });

}

/// This function decrypts a 'Aw11Ciphertext' if the attributes in SK match the policy of CT. If successfull, returns the plaintext data as a Vetor of u8's.
///
/// # Arguments
///
///	* `_gk` - A Global Parameters Key (GK), generated by setup()
///	* `_sk` - A secret user key (SK), associated with a set of attributes.
///	* `_ct` - A Aw11Ciphertext
pub fn decrypt(gk: &Aw11GlobalKey, sk: &Aw11SecretKey, ct: &Aw11Ciphertext) -> Option<Vec<u8>> {
    let _str_attr = sk._attr
        .iter()
        .map(|_values| {
            let (_str, _g2) = _values.clone();
            _str
        })
        .collect::<Vec<_>>();
    if traverse_str(&_str_attr, &ct._policy) == false {
        //println!("Error: attributes in sk do not match policy in ct.");
        return None;
    } else {
        let _pruned = calc_pruned_str(&_str_attr, &ct._policy);
        match _pruned {
            None => {
                //println!("Error: attributes in sk do not match policy in ct.");
                return None;
            }
            Some(_p) => {
                let (_match, _list) = _p;
                let _coeffs = calc_coefficients_str(&ct._policy).unwrap();
                if _match {
                    let _h_g1 = blake2b_hash_g1(gk._g1, &sk._gid);
                    let mut _egg_s = Gt::one();
                    for _current in _list.iter() {
                        let _sk_attr = sk._attr
                            .iter()
                            .filter(|_attr| _attr.0 == _current.to_string())
                            .nth(0)
                            .unwrap();
                        let _ct_attr = ct._c
                            .iter()
                            .filter(|_attr| _attr.0 == _current.to_string())
                            .nth(0)
                            .unwrap();
                        let num = _ct_attr.1 * pairing(_h_g1, _ct_attr.3);
                        let dem = pairing(_sk_attr.1, _ct_attr.2);
                        let _coeff = _coeffs
                            .iter()
                            .filter(|_c| _c.0 == _current.to_string())
                            .map(|_c| _c.1)
                            .nth(0)
                            .unwrap();
                        _egg_s = _egg_s * ((num * dem.inverse()).pow(_coeff));
                    }
                    let _msg = ct._c_0 * _egg_s.inverse();
                    //println!("dec: {:?}", serde_json::to_string(&_msg).unwrap());
                    // Decrypt plaintext using derived secret from cp-abe scheme
                    return decrypt_symmetric(&_msg, &ct._ct);
                } else {
                    println!("Error: attributes in sk do not match policy in ct.");
                    return None;
                }
            }
        }
    }
}
/// private function. finds the value vector of a specific attribute in a vector of various public keys
///
/// # Arguments
///
///	* `_pks` - A vector of Aw11PublicKeys
///	* `_attr` - An attribute
///
fn find_pk_attr(_pks: &Vec<Aw11PublicKey>, _attr: &String) -> Option<(String, bn::Gt, bn::G2)> {
    for _pk in _pks.into_iter() {
        let _pk_attr = _pk._attr
            .clone()
            .into_iter()
            .filter(|_tuple| _tuple.0 == _attr.to_string())
            .nth(0);
        if _pk_attr.is_some() {
            return _pk_attr;
        }
    }
    return None;
}

#[cfg(test)]
mod tests {

    use super::*;

    #[test]
    fn test_and() {
        // global setup
        let _gp = setup();
        // setup attribute authority 1 with
        // a set of two attributes "A" "B" "C"
        let mut att_authority1: Vec<String> = Vec::new();
        att_authority1.push(String::from("A"));
        att_authority1.push(String::from("B"));
        att_authority1.push(String::from("C"));
        let (_auth1_pk, _auth1_msk) = authgen(&_gp, &att_authority1).unwrap();
        // setup attribute authority 1 with
        // a set of two attributes "D" "E" "F"
        let mut att_authority2: Vec<String> = Vec::new();
        att_authority2.push(String::from("D"));
        att_authority2.push(String::from("E"));
        att_authority2.push(String::from("F"));
        let (_auth2_pk, _auth2_msk) = authgen(&_gp, &att_authority2).unwrap();
        // setup attribute authority 1 with
        // a set of two attributes "G" "H" "I"
        let mut att_authority3: Vec<String> = Vec::new();
        att_authority3.push(String::from("G"));
        att_authority3.push(String::from("H"));
        att_authority3.push(String::from("I"));
        let (_auth3_pk, _auth3_msk) = authgen(&_gp, &att_authority3).unwrap();

        // setup a user "bob" and give him some attribute-keys
        let mut att_bob: Vec<String> = Vec::new();
        att_bob.push(String::from("H"));
        att_bob.push(String::from("I"));
        let mut _bob = keygen(&_gp, &_auth3_msk, &String::from("bob"), &att_bob).unwrap();
        // our plaintext
        let _plaintext = String::from("dance like no one's watching, encrypt like everyone is!")
            .into_bytes();
        // our policy
        let _policy = String::from(r#"{"AND": [{"ATT": "H"}, {"ATT": "B"}]}"#);

        let mut _pks: Vec<Aw11PublicKey> = Vec::new();
        _pks.push(_auth3_pk);
        _pks.push(_auth1_pk);

        add_attribute(&_gp, &_auth1_msk, &String::from("B"), &mut _bob);

        // cp-abe ciphertext
        let ct_cp: Aw11Ciphertext = encrypt(&_gp, &_pks, &_policy, &_plaintext).unwrap();
        // and now decrypt again with mathcing sk
        let _matching = decrypt(&_gp, &_bob, &ct_cp).unwrap();
        assert_eq!(_matching, _plaintext);
    }
}
