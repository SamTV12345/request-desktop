import {BodySelector} from "./BodySelector";
import {Outlet} from "react-router-dom";

export const BodyDecider = () => {
    return <div className="h-52">
        <BodySelector/>
        <Outlet/>
    </div>
}
