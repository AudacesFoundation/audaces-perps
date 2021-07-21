module.exports = {
  apps: [
    {
      name: "crank_funding",
      script: "./index.js",
      args: "funding",
      watch: ".",
    },
    {
      name: "crank_funding_extraction",
      script: "./index.js",
      args: "funding-extraction",
      watch: ".",
    },
    {
      name: "crank_liquidate",
      script: "./index.js",
      args: "liquidate",
      watch: ".",
    },
    {
      name: "crank_garbage_collect",
      script: "./index.js",
      args: "garbage-collect",
      watch: ".",
    },
    {
      name: "liquidation_cleanup",
      script: "./index.js",
      args: "liquidation-cleanup",
      watch: ".",
    },
  ],
};
