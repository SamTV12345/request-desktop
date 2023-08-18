import * as ContextMenu from '@radix-ui/react-context-menu';
import {FC} from "react";


type SidebarContextMenuProps = {
    children: React.ReactNode|React.ReactNode[],
    triggerLabel: string
}

export const SidebarContextMenu:FC<SidebarContextMenuProps> = ({children,triggerLabel})=>{


    return <ContextMenu.Root>
        <ContextMenu.Trigger >{triggerLabel}</ContextMenu.Trigger>

        <ContextMenu.Portal>
            <ContextMenu.Content className="bg-basecol p-2 rounded text-sm"  onClick={(e)=>e.stopPropagation()}>
                {children}

                <ContextMenu.Separator />
            </ContextMenu.Content>
        </ContextMenu.Portal>
    </ContextMenu.Root>
}
