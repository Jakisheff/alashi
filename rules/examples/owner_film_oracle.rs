//! Offline film fixture: execute the real rules, without RPC or funds.
use alashi_rules::{actions, constants::*, state::*};

fn execute(goods: u16, sold: u16, tax: u16, action: &str, units: u16) -> String {
    let mut game = Game::default();
    game.phase = Phase::Market;
    game.round = ROUNDS;
    game.sold_this_round = sold;
    game.active_tax_bps = tax;
    let mut faction = Faction::default();
    faction.alive = true;
    faction.cash = 100 * PESO;
    faction.goods = goods;
    let result = match action {
        "sell" => actions::sell(&mut game, &mut faction, units, false),
        "buy" => actions::buy(&mut game, &mut faction, units),
        "pass" => Ok(0),
        _ => panic!("unsupported fixture action"),
    };
    match result {
        Ok(value) => format!("{{\"ok\":true,\"amount\":{value},\"cash_before\":100000000,\"cash_after\":{},\"goods_before\":{goods},\"goods_after\":{},\"sold_after\":{}}}", faction.cash, faction.goods, game.sold_this_round),
        Err(error) => format!("{{\"ok\":false,\"error\":\"{error:?}\"}}"),
    }
}

fn main() {
    let args: Vec<String> = std::env::args().skip(1).collect();
    if args.first().map(String::as_str) == Some("execute") && args.len() == 6 {
        println!(
            "{}",
            execute(
                args[1].parse().unwrap(),
                args[2].parse().unwrap(),
                args[3].parse().unwrap(),
                &args[4],
                args[5].parse().unwrap()
            )
        );
        return;
    }
    assert!(
        args.is_empty(),
        "use: [execute GOODS SOLD TAX ACTION UNITS]"
    );
    print!("[");
    for (index, (goods, sold, tax)) in [(4, 0, 0), (3, 3, 1000), (5, 6, 2000)]
        .into_iter()
        .enumerate()
    {
        if index != 0 {
            print!(",");
        }
        let table = PRICE_TABLE
            .iter()
            .map(u64::to_string)
            .collect::<Vec<_>>()
            .join(",");
        print!("{{\"id\":\"market-{}\",\"round\":{},\"total_rounds\":{},\"phase\":\"market\",\"cash\":100000000,\"goods\":{goods},\"sold_this_round\":{sold},\"tax_bps\":{tax},\"price_table_millions\":[{table}],\"oracle\":{}}}", index + 1, ROUNDS, ROUNDS, execute(goods, sold, tax, "sell", goods));
    }
    println!("]");
}
