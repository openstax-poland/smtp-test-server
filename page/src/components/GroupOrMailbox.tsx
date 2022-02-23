// Copyright 2022 OpenStax Poland
// Licensed under the MIT license. See LICENSE file in the project root for
// full license text.

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
