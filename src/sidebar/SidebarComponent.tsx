import {Box, Button, Stack} from "@mui/material";
import "../App.css";
import {useAPIStore} from "../store/store";
import {SidebarAccordeon} from "../components/SidebarAccordeon";
import {Folder} from "@mui/icons-material";
export const SidebarComponent = ()=>{
    const collections = useAPIStore(state=>state.collections)

    return (
            <div className="sidebar">
                {
                    collections.map((c, i)=>{
                        return (
                            <Box sx={{padding: '20px'}}>
                                <Stack key={i} direction="row" spacing={2}>
                                    <SidebarAccordeon key={c.id} collection={c}/>
                                </Stack>
                            </Box>
                        )
                    })
                }

            </div>
    )
}
