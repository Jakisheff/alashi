use alashi_rules::{sim::Simulator, constants::*, Phase, VoteChoice};
use alashi_rules::anchor_lang::prelude::Pubkey;
use serde_json::{json, Value};
fn base(cash:[u64;3], influence:[u16;3], phase:Phase)->Simulator {
    let mut s=Simulator::new(1, 0, 1, 0);
    for i in 0..3 {s.join(Pubkey::new_from_array([i as u8+1;32]), &format!("U{i}")).unwrap();}
    s.game.epoch=EPOCH_90S; s.game.vote_weight_mode=VOTE_WEIGHT_CONTRIB;
    s.game.round=6; s.game.phase=phase; s.game.phase_ends_at=0;
    for i in 0..3 {s.factions[i].cash=cash[i]*PESO; s.factions[i].influence=influence[i];}
    s
}
fn finish(mut s:Simulator)->Value {
    if s.game.phase==Phase::Action {s.advance_with_card(100,LAW_STATUS_QUO).unwrap();}
    s.advance_with_card(200,LAW_STATUS_QUO).unwrap();
    assert_eq!(s.game.phase,Phase::Finished);
    let p=alashi_rules::compute_settlement_epoch(&s.game,&s.factions,100*PESO,0,true,false).unwrap();
    let line=p.lines.iter().find(|x| x.idx==0).unwrap();
    json!({"payout":line.total,"rank":line.rank,"factory_bonus":line.factory_bonus,
        "cash":s.factions.iter().map(|x|x.cash).collect::<Vec<_>>(),
        "influence":s.factions.iter().map(|x|x.influence).collect::<Vec<_>>()})
}
fn main(){
 let mut rows=vec![];
 // Eight Law snapshots, with role authority and beneficiaries varied independently.
 let fixtures=[
  ([10,30,20],[3,3,1],LAW_SUBSIDY_POOR),
  ([30,10,20],[3,3,1],LAW_SUBSIDY_POOR),
  ([10,30,20],[3,3,1],LAW_SUBSIDY_RICH),
  ([30,10,20],[3,3,1],LAW_SUBSIDY_RICH),
  ([10,30,20],[2,3,1],LAW_SUBSIDY_POOR),
  ([30,10,20],[2,3,1],LAW_SUBSIDY_POOR),
  ([10,30,20],[2,3,1],LAW_SUBSIDY_RICH),
  ([30,10,20],[2,3,1],LAW_SUBSIDY_RICH),
 ];
 for (i,(cash,inf,card)) in fixtures.into_iter().enumerate(){
  let mut s=base(cash,inf,Phase::Law); s.game.law_card=card;
  for f in &mut s.factions {f.acted_stamp=(6u16 << 3) | Phase::Action as u16;}
  let holder=if inf[0]>=inf[1] {0} else {1}; s.game.president=s.factions[holder].wallet;
  // Other decisions are fixed, not inferred from labels or earlier model responses.
  s.vote(1,VoteChoice::Abstain).unwrap(); s.vote(2,VoteChoice::Yes).unwrap();
  let mut options=vec![];
  for (id,choice) in [("yes",VoteChoice::Yes),("no",VoteChoice::No),("abstain",VoteChoice::Abstain)]{
   let mut branch=s.clone(); branch.vote(0,choice).unwrap();
   options.push(json!({"id":id,"outcome":finish(branch)}));
  }
  if holder==0 {let mut branch=s.clone();branch.veto(0).unwrap();options.push(json!({"id":"veto","outcome":finish(branch)}));}
  rows.push(json!({"id":format!("L{}",i+1),"phase":"Law","round":6,"actor":0,
   "holder":holder,"cash":cash.map(|x|x*PESO),"influence":inf,"goods":[0,0,0],
   "card":card,"other_votes":[null,"abstain","yes"],"options":options}));
 }
 for (i,(cash,inf)) in [([60,28,10],[2,3,1]),([32,28,10],[2,3,1]),([30,27,10],[2,2,1]),([20,30,10],[2,3,1])].into_iter().enumerate(){
  let s=base(cash,inf,Phase::Action);
  let mut options=vec![json!({"id":"pass","outcome":finish(s.clone())})];
  let mut branch=s.clone();branch.produce(0).unwrap();options.push(json!({"id":"produce","outcome":finish(branch)}));
  for amount in [5,10] {for to in [1,2] {let mut b=s.clone();b.bribe(0,to,amount*PESO).unwrap();
   options.push(json!({"id":format!("transfer_{amount}_{to}"),"to":to,"amount":amount*PESO,"outcome":finish(b)}));}}
  rows.push(json!({"id":format!("A{}",i+1),"phase":"Action","round":6,"actor":0,
    "holder":null,"cash":cash.map(|x|x*PESO),"influence":inf,"goods":[0,0,0],"card":0,
    "other_votes":[null,"abstain","abstain"],"options":options}));
 }
 assert_eq!(rows.len(),12);
 assert!(rows.iter().all(|r|r["options"].as_array().unwrap().iter().all(|o|o["outcome"]["payout"].as_u64().is_some())));
 println!("{}",serde_json::to_string_pretty(&rows).unwrap());
}
