const express = require("express");
const cors = require("cors");
const dotenv = require("dotenv");
dotenv.config();

const PORT = 5000;
const JWT_SECRET = process.env.JWT_SECRET;

const app = express();
app.use(express.json());
app.use(cors());

app.get("/getSecret", (req, res) => {
    res.json({success: true, jwt: JWT_SECRET});
}); 

app.listen(PORT, () => {
    console.log(`Server is currently listening to port ${PORT}`);
})