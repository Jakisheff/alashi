//! Read-only experimental interface: legal candidate probes on cloned state.
use alashi_rules::{sim::Simulator, state::{Phase, VoteChoice}, constants::*};
use crate::{runner::{apply_market,apply_action,apply_law},strategies::{MarketAction,ActionAction,LawAction}};
use serde_json::{Value,json};
fn probe(sim:&Simulator,i:usize,v:Value)->Option<Value>{
 let mut b=sim.clone();let a=v["action"].as_str().unwrap();
 let log=match sim.game.phase{
  Phase::Market=>apply_market(&mut b,i,&match a{
   "pass"=>MarketAction::Pass,"sell"=>MarketAction::Sell(v["units"].as_u64().unwrap() as u16),
   "buy"=>MarketAction::Buy(v["units"].as_u64().unwrap() as u16),
   "sell_credit"=>MarketAction::SellCredit(v["units"].as_u64().unwrap() as u16),_=>unreachable!()}),
  Phase::Action=>apply_action(&mut b,i,&match a{
   "pass"=>ActionAction::Pass,"produce"=>ActionAction::Produce,"shuttle"=>ActionAction::Shuttle,"donkey"=>ActionAction::Donkey,
   "bribe"=>ActionAction::Bribe{to:v["to"].as_u64().unwrap() as usize,amount:v["amount"].as_u64().unwrap()},
   "roof"=>ActionAction::Roof{to:v["to"].as_u64().unwrap() as usize,tariff:v["tariff"].as_u64().unwrap() as u8},
   "bid"=>ActionAction::Bid(v["amount"].as_u64().unwrap()),_=>unreachable!()}),
  Phase::Law=>{let wallet=b.factions[i].wallet;apply_law(&mut b,i,&match a{
   "pass"=>LawAction::Pass,"veto"=>LawAction::Veto,"vote_yes"=>LawAction::Vote(VoteChoice::Yes),
   "vote_no"=>LawAction::Vote(VoteChoice::No),"vote_abstain"=>LawAction::Vote(VoteChoice::Abstain),_=>unreachable!()},&wallet)},_=>unreachable!()};
 if !log.ok{return None;}
 let f=&b.factions[i];let mut result=v;
 result["after_own_action"]=json!({"cash":f.cash,"goods":f.goods,"influence":f.influence,"promissory":f.promissory,"own_bid":f.bid,"roof_armed":f.roof_armed,"roof_tariff":f.roof_tariff});
 Some(result)
}
pub fn public_view(sim:&Simulator,i:usize)->Value{
 let before=sim.state_bytes();let f=&sim.factions[i];let mut options=vec![];
 let mut add=|v:Value|{if let Some(valid)=probe(sim,i,v){if !options.contains(&valid){options.push(valid);}}};
 add(json!({"action":"pass"}));
 match sim.game.phase{
  Phase::Market=>{
   for units in 1..=f.goods{for action in ["sell","sell_credit"]{add(json!({"action":action,"units":units}));}}
   for units in 1..=(f.cash/PESO).min(u16::MAX as u64){
    let v=json!({"action":"buy","units":units});if probe(sim,i,v.clone()).is_none(){break;}add(v);
   }
  },
  Phase::Action=>{
   for action in ["produce","donkey","shuttle"]{add(json!({"action":action}));}
   for to in 0..sim.factions.len(){if to==i{continue;}
    for amount in (BRIBE_PRICE..=f.cash).step_by(BRIBE_PRICE as usize){add(json!({"action":"bribe","to":to,"amount":amount}));}
    if f.cash>0{add(json!({"action":"bribe","to":to,"amount":f.cash}));}
    for tariff in [ROOF_BLACK,ROOF_RED]{add(json!({"action":"roof","to":to,"tariff":tariff}));}
   }
   if sim.game.round==AUCTION_ROUND && f.bid==0{
    for amount in (PESO..=f.cash).step_by(PESO as usize){add(json!({"action":"bid","amount":amount}));}
    if f.cash>0{add(json!({"action":"bid","amount":f.cash}));}
   }
  },
  Phase::Law=>{for action in ["vote_yes","vote_no","vote_abstain","veto"]{add(json!({"action":action}));}},_=>unreachable!()
 }
 let mut tie_order:Vec<usize>=(0..sim.factions.len()).collect();tie_order.sort_by_key(|j|sim.factions[*j].wallet);
 let holder=sim.factions.iter().position(|f|f.wallet==sim.game.president);
 let rows:Vec<Value>=sim.factions.iter().enumerate().map(|(j,f)|json!({"idx":j,"cash":f.cash,"goods":f.goods,"influence":f.influence,"promissory":f.promissory,"hard":f.hard,"alive":f.alive,"roof_armed":f.roof_armed,"roof_tariff":f.roof_tariff,"grey_goods":f.grey_goods,"own_bid":if j==i{Some(f.bid)}else{None},"voted":f.voted_stamp==sim.game.stamp(),"vote":if f.voted_stamp==sim.game.stamp(){Some(format!("{:?}",f.vote))}else{None}})).collect();
 assert_eq!(before,sim.state_bytes(),"Read-only candidate generation changed engine state");
 json!({"rows":rows,"holder":holder,"tie_order":tie_order,"candidates":options,"active_subsidy_goods":sim.game.active_subsidy_goods,"bank":sim.game.entry_fee*sim.factions.len() as u64+sim.game.prize_pot,"license_sold":sim.game.license_sold,"license_holder":sim.factions.iter().position(|f|f.wallet==sim.game.license_holder),"epoch":sim.game.epoch})
}
#[cfg(test)]mod tests{
 use super::*;use alashi_rules::anchor_lang::prelude::Pubkey;
 #[test]fn probes_are_read_only_and_hide_private_values(){
  let mut s=Simulator::new(1,10*PESO,10,0);for j in 0..3{s.join(Pubkey::new_from_array([j+1;32]),"u").unwrap();}
  s.game.epoch=EPOCH_90S;s.game.round=4;s.game.phase=Phase::Action;s.game.license_yield=51723149;
  for f in &mut s.factions{f.cash=13*PESO;}
  s.factions[1].bid=11223344;
  let original=s.state_bytes();let v=public_view(&s,0);assert_eq!(s.state_bytes(),original);
  assert!(!v.to_string().contains("51723149"));assert!(!v.to_string().contains("11223344"));
  assert!(v["candidates"].as_array().unwrap().iter().any(|x|x["action"]=="bid"&&x["amount"]==13*PESO));
  assert!(v["candidates"].as_array().unwrap().iter().all(|x|x["to"]!=0));
  s.game.phase=Phase::Law;s.game.law_card=1;s.game.president=s.factions[1].wallet;
  assert!(!public_view(&s,0)["candidates"].as_array().unwrap().iter().any(|x|x["action"]=="veto"));
 }
}
