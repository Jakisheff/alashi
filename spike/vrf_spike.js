const { Keypair, LAMPORTS_PER_SOL } = require("@solana/web3.js");
const sb = require("@switchboard-xyz/on-demand");

async function main() {
  console.log("[0] loadEnv (solana CLI config: devnet + id.json)");
  const cfg = await sb.AnchorUtils.loadEnv();
  const conn = cfg.connection;
  const kp = cfg.keypair;
  console.log("wallet:", kp.publicKey.toBase58());
  const bal = await conn.getBalance(kp.publicKey);
  console.log("balance:", bal / LAMPORTS_PER_SOL, "SOL");
  if (bal < 20_000_000) {
    console.log("НЕДОСТАТОЧНО SOL ДЛЯ СПАЙКА (нужно ~0.02), выход");
    process.exit(2);
  }

  console.log("\n[1] sb program + queue");
  const sbProgram = cfg.program;
  const queue = await sb.getDefaultQueue(conn.rpcEndpoint);
  console.log("queue:", queue.toBase58());

  console.log("\n[2] create randomness account");
  const rngKp = Keypair.generate();
  const balBefore = await conn.getBalance(kp.publicKey);
  const [randomness, createIx] = await sb.Randomness.create(
    sbProgram,
    rngKp,
    queue
  );
  const createTx = await sb.asV0Tx({
    connection: conn,
    ixs: [createIx],
    payer: kp.publicKey,
    signers: [kp, rngKp],
    computeUnitPrice: 75_000,
    computeUnitLimitMultiple: 1.3,
  });
  const createSig = await conn.sendTransaction(createTx);
  await conn.confirmTransaction(createSig, "confirmed");
  console.log("create sig:", createSig);

  console.log("\n[3] commit");
  const commitIx = await randomness.commitIx(queue);
  const commitTx = await sb.asV0Tx({
    connection: conn,
    ixs: [commitIx],
    payer: kp.publicKey,
    signers: [kp],
    computeUnitPrice: 75_000,
    computeUnitLimitMultiple: 1.3,
  });
  const commitSig = await conn.sendTransaction(commitTx);
  await conn.confirmTransaction(commitSig, "confirmed");
  console.log("commit sig:", commitSig);

  console.log("\n[4] ждём генерации (3с) и reveal");
  await new Promise((r) => setTimeout(r, 3000));
  const revealIx = await randomness.revealIx();
  const revealTx = await sb.asV0Tx({
    connection: conn,
    ixs: [revealIx],
    payer: kp.publicKey,
    signers: [kp],
    computeUnitPrice: 75_000,
    computeUnitLimitMultiple: 1.3,
  });
  const revealSig = await conn.sendTransaction(revealTx);
  await conn.confirmTransaction(revealSig, "confirmed");
  console.log("reveal sig:", revealSig);

  console.log("\n[5] читаем значение");
  const data = await randomness.loadData();
  console.log("randomness account state:", JSON.stringify(data, (k, v) =>
    typeof v === "bigint" ? v.toString() : v
  ).slice(0, 600));

  const balAfter = await conn.getBalance(kp.publicKey);
  const rent = await conn.getBalance(rngKp.publicKey);
  console.log("\n=== ЦИФРЫ СПАЙКА ===");
  console.log("потрачено всего (create+commit+reveal+fee):", balBefore - balAfter, "lamports");
  console.log("из них rent randomness-аккаунта:", rent, "lamports (одноразово, аккаунт переиспользуется)");
  console.log("чистая цена цикла commit+reveal:", balBefore - balAfter - rent, "lamports");
  console.log("=== СПАЙК ОК: [П3] devnet работает, [П1] цена выше ===");
}

main().catch((e) => {
  console.error("SPIKE FAILED:", e.message || e);
  process.exit(1);
});
