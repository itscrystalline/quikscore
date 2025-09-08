const express = require("express");
const cors = require("cors");
const dotenv = require("dotenv");
dotenv.config();

const PORT = 5000;

const ADMIN_USERNAME = process.env.ADMIN_USERNAME;
const ADMIN_PASSWORD = process.env.ADMIN_PASSWORD;

const app = express();
app.use(express.json());
app.use(cors());

app.post("/login", (req, res) => {
    const { username, password } = req.body;

    if(username == ADMIN_USERNAME && password == ADMIN_PASSWORD) {
        res.json({success: true, message: "SUCCESS!"});
        return;
    }
    res.json({success: false, message: "Invalid Credentials, please contact admin"});
})

app.listen(PORT, () => {
    console.log(`Server is currently listening to port ${PORT}`);
})