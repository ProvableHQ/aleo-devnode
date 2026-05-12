// Copyright (C) 2019-2026 Provable Inc.
// This file is part of the Leo library.

// The Leo library is free software: you can redistribute it and/or modify
// it under the terms of the GNU General Public License as published by
// the Free Software Foundation, either version 3 of the License, or
// (at your option) any later version.

// The Leo library is distributed in the hope that it will be useful,
// but WITHOUT ANY WARRANTY; without even the implied warranty of
// MERCHANTABILITY or FITNESS FOR A PARTICULAR PURPOSE. See the
// GNU General Public License for more details.

// You should have received a copy of the GNU General Public License
// along with the Leo library. If not, see <https://www.gnu.org/licenses/>.

use anyhow::Result;
use clap::Parser;

/// All 50 pre-funded development accounts for the built-in genesis block (address, private_key).
pub const FUNDED_ACCOUNTS: &[(&str, &str)] = &[
    ("aleo1rhgdu77hgyqd3xjj8ucu3jj9r2krwz6mnzyd80gncr5fxcwlh5rsvzp9px", "APrivateKey1zkp8CZNn3yeCseEtxuVPbDCwSyhGW6yZKUYKfgXmcpoGPWH"),
    ("aleo1s3ws5tra87fjycnjrwsjcrnw2qxr8jfqqdugnf0xzqqw29q9m5pqem2u4t", "APrivateKey1zkp2RWGDcde3efb89rjhME1VYA8QMxcxep5DShNBR6n8Yjh"),
    ("aleo1ashyu96tjwe63u0gtnnv8z5lhapdu4l5pjsl2kha7fv7hvz2eqxs5dz0rg", "APrivateKey1zkp2GUmKbVsuc1NSj28pa1WTQuZaK5f1DQJAT6vPcHyWokG"),
    ("aleo12ux3gdauck0v60westgcpqj7v8rrcr3v346e4jtq04q7kkt22czsh808v2", "APrivateKey1zkpBjpEgLo4arVUkQmcLdKQMiAKGaHAQVVwmF8HQby8vdYs"),
    ("aleo1p9sg8gapg22p3j42tang7c8kqzp4lhe6mg77gx32yys2a5y7pq9sxh6wrd", "APrivateKey1zkp3J6rRrDEDKAMMzSQmkBqd3vPbjp4XTyH7oMKFn7eVFwf"),
    ("aleo1l4z0j5cn5s6u6tpuqcj6anh30uaxkdfzatt9seap0atjcqk6nq9qnm9eqf", "APrivateKey1zkp6w2DLUBBAGTHUK4JWqFjEHvqhTAWDB5Ex3XNGByFsWUh"),
    ("aleo1aukf3jeec42ssttmq964udw0efyzt77hc4ne93upsu2plgz0muqsg62t68", "APrivateKey1zkpEBzoLNhxVp6nMPoCHGRPudASsbCScHCGDe6waPRm87V1"),
    ("aleo1y4s2sjw03lkg3htlcpg683ec2j9waprljc657tfu4wl6sd67juqqvrg04a", "APrivateKey1zkpBZ9vQGe1VtpSXnhyrzp9XxMfKtY3cPopFC9ZB6EYFays"),
    ("aleo1xh2lnryvtzxcvlz8zzgranu6yldaq5257cac44de4v0aasgu45yq3yk3yv", "APrivateKey1zkpHqcqMzArwGX3to2x1bDVFDxo7uEWL4FGVKnstphnybZq"),
    ("aleo19ljgqpwy98l9sz4f6ly028rl8j8r4grlnetp9e2nwt2xwyfawuxq5yd0tj", "APrivateKey1zkp6QYrYZGxnDmwvQSg7Nw6Ye6WUeXHvs3wtj5Xa9LArc7p"),
    ("aleo1s2tyzgqr9p95sxtj9t0s38cmz9pa26edxp4nv0s8mk3tmdzmqgzsctqhxg", "APrivateKey1zkp9AZwPkk4gYUCRtkaX5ZSfBymToB7azBJHmJkSvfyfcn4"),
    ("aleo1sufp275hshd7srrkxwdf7yczmc89em6e5ly933ftnaz64jlq8qysnuz88d", "APrivateKey1zkp2jCDeE8bPnKXKDrXcKaGQRVfoZ1WFUiVorbTwDrEv6Cg"),
    ("aleo1mwcjkpm5hpqapnsyddnwtmd873lz2kpp6uqayyuvstr4ky2ycv9sglne5m", "APrivateKey1zkp7St3ztS3cag91PpyQbBffTc8YLmigCGB97Sf6bkQwvpg"),
    ("aleo1khukq9nkx5mq3eqfm5p68g4855ewqwg7mk0rn6ysfru73krvfg8qfvc4dt", "APrivateKey1zkpGcGacddZtDLRc8RM4cZb6cm3GoUwpJjSCQcf2mfeY6Do"),
    ("aleo1masuagxaduf9ne0xqvct06gpf2tmmxzcgq5w2el3allhu9dsv5pskk7wvm", "APrivateKey1zkp4ZXEtPR4VY7vjkCihYcSZxAn68qhr6gTdw8br95vvPFe"),
    ("aleo10w89dpq8tqzeghq35nxtk2k66pskxm8vhrdl3vx6r4j9hkgf2qqs3936q6", "APrivateKey1zkpH7XEPZDUrEBnMtq1JyCR6ipwjFQ5jiHnTCe7Z7heyxff"),
    ("aleo1sfu3p7g8rppusft8re7v88ujjhz5sx6pwc5609vdgnr0pdmhkyyqrrsjkm", "APrivateKey1zkpA9S3Epe8mzDnMuAXBmdxyRXgB8yp7PuMrs2teh8xNcVe"),
    ("aleo1ry0wc384qthrdna5xtzsjqvxg42zwfezpna6keeqa6netv3qmyxszhh8z8", "APrivateKey1zkp5neB5iVnXMTrR6y8P6wndGE9xWhQzBf3Qoht9yQ17a5o"),
    ("aleo1ps4dhhfn5vgfj9lyjra2xnv9a8cc2a2l9jnr585h6tvj4gnlqgfqyszcv3", "APrivateKey1zkp4u1cUbvkC2r3n3Gz3eNzth1TvffGbFeLgaYyk8efsT4e"),
    ("aleo15a34a3dtpj879frvndndp0605vqnxsfdedwyrtu5u6xfd7fv5ufqryavc4", "APrivateKey1zkpBs9zc9FChKZAkoHsf1TERcd9EQhe43NS1xuNSnyJSH1K"),
    ("aleo1mpn4enrfm2dqjg8lqh09t2zcatkujq3qr3kq8kcnrd7uaqrc3c9qngcp5l", "APrivateKey1zkp3sh4dSfCXd9g86DGHx6PAQG7WrMxE8bMbJxCrpPKSUw3"),
    ("aleo1axy39ux5lhaypf039zp7fuhg57qkfqtafu2fa3e2vwgqugeq05qsm2kfl4", "APrivateKey1zkpApK3vKdDDwbf62K5Mh7JsPNksud3ypZEXvuoYPcazStS"),
    ("aleo1zzpl369camggvj5qm2nhnpfhe3epcera3xvdra4ze7scg35zmuzsl7kwyh", "APrivateKey1zkp2uS6cU4M4J8z2fE3uMuQHkg87AgrMnDQ8NZzGAnpiEXm"),
    ("aleo1j2fhcu3qkvn4k0vrf53jmuv8d0fz5guz9tzdy0egjjjttdhsxszqfdfwfk", "APrivateKey1zkp8za2Nc39VHQFzBQFH6rhKuB9LqPaoVw1SgUPG8pSGAAn"),
    ("aleo1jqfyapkxkx3hk05mjzky9cqxjr5yz33fwfqujpd0uy5zxxwfkvqskcffhq", "APrivateKey1zkp4JjfHAVD9d6S9n8FYpVnapkJ3yfiyPaQNnAqsuexUQcU"),
    ("aleo1sap6x2ndmmwm8t6z568hc33u8ayynw2ha9u9pck7wvrrd0e7k58sty954x", "APrivateKey1zkpFT2mMYvZ7TPzjkCGH5F64itzRBwjscqqqezx6AaPnqxF"),
    ("aleo19afl0wwru8ws7g7c727j27x0e8r7sa5gkjz6jv37sr4ujlm5sqzs2qt8wz", "APrivateKey1zkpJcSh3d66dxtTvaA1b9P9xAdUXMKnWsQSkhmZRDpEUJYr"),
    ("aleo1ypkmme72un8k4dzaj0z6ha2skz9adskscwf75eul27j5e7lu6s9q0pu9qx", "APrivateKey1zkpAy7c3uea5yuvjkuN9eqGSoBJDHpE33yCe5qu7u2JbVmZ"),
    ("aleo1av8367kf2dre7mwpuyhcg7wrhtvs27usa53fn8uwmd5x7r2gsyrs9084ge", "APrivateKey1zkp3GUSi7FQW7FgLyPp77A45CvDjZFwdqgWzLzkxbB3GXKD"),
    ("aleo1y7swdgs3zav50a0r0sx8us4tqycp67kc9ypnml5eqjfpypk7cgqszf6dvn", "APrivateKey1zkp5rtsaS8tZwZVrf5PwQwnfvcm7Q4UntrcXXiwYTMRuz9T"),
    ("aleo16dyj3gxptzm5vgfyxlv2s6869ftfdxwpl2hm9r09uqk69kcjwvfqqpu2pm", "APrivateKey1zkpDPHco2BZh95YCD2eZc44LZ8YfuZq85qfULBVgUB6SouE"),
    ("aleo1ftv0e670e2nezrdajxg947vn3el4cgjt47nghleuw5a7dja9dyrszr5jhp", "APrivateKey1zkp2jaPbqqFLXiTr92CSDEqevwzaVsj4MbC43apRKFXnWSd"),
    ("aleo1d7wfgrgtk75g8m08d7f8jmyk6f8kg9w8jpm7l97hyx0kf6l8asrsdnhzzz", "APrivateKey1zkpEycEZpddReHV4UExGpWSQZUCau2g6K2HP1jQnCSPUAL9"),
    ("aleo1hcaq72hw8tf7ms4qfppefklwdr32ud3nngu88wnx6dq67dzgl58scpsg8v", "APrivateKey1zkp9gJgMLBiVKVRSSqbRDQFcKm84sQbJF9wqWzcSnXVw1we"),
    ("aleo1szkl7zn07mgd44qpg43wk3sd5emggc3w5frd3wwms9c4rwklygzs7mn4x9", "APrivateKey1zkp7jX54qsuFZ5Ks2DJhPzx6io2EM2CZhTYA3XU2XfXt2rr"),
    ("aleo1sekqksjqnmhpyca5juxxghrujck8dm9lrhp70nrsp9a7hd4sxczswyrnjj", "APrivateKey1zkpDyVQ7mGpb48oS44Zee1gPvA19ng98S2MRCCCcsh7Av8r"),
    ("aleo18y86x2qvjsg0tay6fj9cjqejhvv45wnd53a66tax6j3zrxu9gupsh7e86v", "APrivateKey1zkp5grVqsMuASdVowmgsK5CCBjz9dAqAw2K1szc4jPC15EN"),
    ("aleo1rnhvu0f4m6ymwegyemqyt7hfsqqaqxpn29l7jafvyuw60nz9ygysu0jn23", "APrivateKey1zkp5s39kX98KZmm4vdfhHuWhMbP5mVCREFRZuT9GGduzb6x"),
    ("aleo192e2mn3lmav8csm0krjn0va5v9nur6pr03y8vepe09xc5qummvxq338czu", "APrivateKey1zkp642Mn8JgLFC4C5JGdy73VMg5skFoAj5dmaxKs2zTnDbN"),
    ("aleo1mjwjwe67rzs7w08psynya7pc3q4shpyggz92xksy4rumfzfmdc9qnrk4pl", "APrivateKey1zkpFisano5hmJvALiVkgVcxVRL61aS8jz2CwFeAQQS9j6Qu"),
    ("aleo1ffdnchytga8nuzg557h8cx0h89xlddxhucxdphtg8k8v8tn7zuysxa893d", "APrivateKey1zkp6gB8LRzRs1chkWMnk7ffAytADkdZEDxggqEEAdV1qQBC"),
    ("aleo16w6zw8glrj08psy5nwumsed7kmxwxk8paq2d92m7dk2uerwzuqqsvkamnk", "APrivateKey1zkp7mXyitjWX5hXUznSKpfEMUqdMVzCwG728Nzf1R1axRzL"),
    ("aleo1dfystswcj4j0k8nckast6ylexrwuhv42ysldx7x03q0uch259ypsfml8h7", "APrivateKey1zkp6WadY8WbPq2or8YMJDFyS9c6HHyoJEif567i4SqVv2h8"),
    ("aleo1j97mw86ytd6v6zl76qju6dm9ee3t0zjdsaydph7dtweusm6vavqsmswgzg", "APrivateKey1zkpJMcEf79UR7n2W5rzLrkeg1hTrWHQyNJcvKmhsJWkzTsN"),
    ("aleo17g5m4scr8x8yndx6spsmnu5wk45sgwe4w2la8gnxu0zjn7cfssxq2sxsef", "APrivateKey1zkpEe33jVdjakXiQcKfJUf9fyVaTMxHJcuipXmAWgy55TFQ"),
    ("aleo1hkuf3ypfym59m23c8jmw97yxpldsk2ycc9rc2x0eaaxd9x6fmvqqpfg08d", "APrivateKey1zkpGTtLxB3mUbew6mkP3D7tqVveVEZYYKmgVun1CyXJcXmF"),
    ("aleo1e9xrl7ummq63d6zay60w5klqkfhya5kwcwa757hqzgujkk9ddcgqnufluq", "APrivateKey1zkp7aPCqYDux1n8DdLGPFNqkoHjSjpkoSaiVpKcf4Xpgz3B"),
    ("aleo1u44retdlrhptya55qgxly0um6ydajrz2mhthxtyukzekctxtag9sdcy0cm", "APrivateKey1zkpDbTD13qeLjMA6ympzFFo1Z2mnUHg8DRAaSKE7qCWW79f"),
    ("aleo1wqcgpvt58d03nl34vhx3kc0v4jh04f6alvt5s38q3jhdq3shqcxq6g5v70", "APrivateKey1zkpBfe9853NcMnagBeYpimPrT5ZN8fYi8aMv45rxkxbm7Gn"),
    ("aleo1s77df89g2km8urqvanvthhuxyw9d32plmjvewthm7cjhnezxtygq78mrdn", "APrivateKey1zkpG22T5KZGDE54HmVCp91vhzNrzb7HihynmUyZ4DX9Ngj2"),
];

/// List all pre-funded accounts for the built-in genesis block.
#[derive(Parser, Debug)]
#[group(id = "accounts_args")]
pub struct Accounts;

impl Accounts {
    pub fn execute(self) -> Result<()> {
        let sep = "═".repeat(70);
        println!("{sep}");
        println!("  Pre-funded Accounts ({} total — built-in genesis only)", FUNDED_ACCOUNTS.len());
        println!("{sep}");
        for (i, (address, private_key)) in FUNDED_ACCOUNTS.iter().enumerate() {
            println!("  ({i}) {address}");
            println!("      {private_key}");
            println!();
        }
        println!("⚠  These are development keys. Never use them in production.");
        println!("  Using a custom genesis? Query block 0: GET http://localhost:3030/testnet/block/0");
        println!("{sep}");
        Ok(())
    }
}
