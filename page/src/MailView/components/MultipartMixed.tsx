// Copyright 2022 OpenStax Poland
// Licensed under the MIT license. See LICENSE file in the project root for
// full license text.

import * as React from 'react'

import { Message, Multipart } from '~/src/data'

import MultipartPart from './MultipartPart'

export interface Props {
    message: Message
    part?: string
    data: Multipart
}

export default function MultipartMixed({ message, part, data }: Props) {
    return <div className="multipart mixed">
        {data.parts.map((data, index) => <MultipartPart
            key={index}
            message={message}
            part={`${part ?? ''}/${index}`}
            contentType={data.contentType}
            />
        )}
    </div>
}
