import { Group as GroupData, Mailbox as MailboxData } from '~/src/data'

import Group from './Group'
import Mailbox from './Mailbox'

interface Props {
    group: GroupData | MailboxData
    format?: 'short' | 'full'
}

export default function GroupOrMailbox({ group, format }: Props) {
    if ('members' in group) {
        return <Group group={group} />
    } else {
        return <Mailbox mailbox={group} format={format} />
    }
}
