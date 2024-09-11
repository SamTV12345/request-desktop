import {SidebarComponent} from "../../sidebar/SidebarComponent";
import {Outlet} from "react-router-dom";

export const CollectionRouter = ()=>{
    return <><SidebarComponent/>
        <div className="main-panel">
            <Outlet/>
        </div>
    </>
}