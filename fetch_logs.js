const fs = require('fs');

async function fetchLogs() {
  try {
    const res = await fetch("https://api.github.com/repos/GeneraBlack/BlackTeaSpeak/actions/jobs/77549001037/logs");
    const text = await res.text();
    fs.writeFileSync("github_log.txt", text);
    console.log("Log saved to github_log.txt!");
  } catch (e) {
    console.error(e);
  }
}

fetchLogs();
