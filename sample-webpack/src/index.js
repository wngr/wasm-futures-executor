import init, { start } from 'sample';

async function run() {
  await init();

  const res = await start();
  console.log("result", res);
}
run();
