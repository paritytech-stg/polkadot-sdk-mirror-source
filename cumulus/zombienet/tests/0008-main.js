// Allows to manually submit extrinsic to collator.
// Usage:
// zombienet-linux -p native spwan 0008-parachain-extrinsic-gets-finalized.toml
// node 0008-main.js <path-to-generated-tmpdir/zombie.json>

global.zombie = null

const fs = require('fs');
const test = require('./0008-transaction_gets_finalized.js');

if (process.argv.length == 2) {
    console.error('Path to zombie.json (generated by zombienet-linux spawn command shall be given)!');
    process.exit(1);
}

let networkInfo = JSON.parse(fs.readFileSync(process.argv[2]));

test.run("charlie", networkInfo).then(process.exit)