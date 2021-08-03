require("dotenv").config();
const { spawn } = require("child_process");
const fetch = require("node-fetch");
const publicIp = require("public-ip");

const FEE_PAYER = process.env.FEE_PAYER; // Path to your wallet file

const ENDPOINT = process.env.ENDPOINT; // RPC Endpoint

const SLACK_URL = process.env.SLACK_URL; // Slack URL for notification (optional)

const MARKETS = [
  {
    market: "475P8ZX3NrzyEMJSFHt9KCMjPpWBWGa6oNxkWcwww2BR",
    programId: "perpke6JybKfRDitCmnazpCrGN5JRApxxukhA9Js6E6",
    name: "BTC/USDC",
  },
  {
    market: "3ds9ZtmQfHED17tXShfC1xEsZcfCvmT8huNG79wa4MHg",
    programId: "perpke6JybKfRDitCmnazpCrGN5JRApxxukhA9Js6E6",
    name: "ETH/USDC",
  },
  {
    market: "jeVdn6rxFPLpCH9E6jmk39NTNx2zgTmKiVXBDiApXaV",
    programId: "perpke6JybKfRDitCmnazpCrGN5JRApxxukhA9Js6E6",
    name: "SOL/USDC",
  },
];

const SERVICES = [
  "funding-extraction",
  "funding",
  "garbage-collect",
  "liquidate",
  "liquidation-cleanup",
];

const postSlack = async (message) => {
  if (!SLACK_URL || SLACK_URL == "") return;
  const ip = await publicIp.v4();
  message += ` - Machine ${ip}`;
  try {
    let response = await fetch(SLACK_URL, {
      method: "POST",
      body: JSON.stringify({ text: message }),
      headers: {
        "Content-Type": "application/json",
      },
    });
    if (!response.ok) {
      throw new Error("Error sending message to Slack");
    }
    return response;
  } catch (err) {
    console.warn(err);
  }
};

const crank = () => {
  const service = process.argv.slice(2)[0];
  if (!SERVICES.includes(service))
    throw new Error(`Invalid service passed in argument ${service}`);
  for (let market of MARKETS) {
    console.log(`Spawning ${market.market} ${service}`);
    const worker = spawn("../target/release/./perps-crank", [
      "--url",
      ENDPOINT,
      "--market",
      market.market,
      "--program-id",
      market.programId,
      "--fee-payer",
      FEE_PAYER,
      service,
    ]);

    worker.stdout.on("data", (data) => {
      console.log(`stdout: ${data}`);
    });
    worker.stderr.on("data", async (data) => {
      await postSlack(`market ${market} - service ${service} - ${data}`);
      console.log(`stderr: ${data}`);
    });
    worker.on("error", async (error) => {
      await postSlack(`market ${market} - service ${service} - error`);
      console.log(`error: ${error.message}`);
    });
    worker.on("close", async (code) => {
      await postSlack(
        `child process exited with code ${code} - market ${market} - service ${service}`
      );
      console.log(`child process exited with code ${code}`);
    });
  }
};

crank();
