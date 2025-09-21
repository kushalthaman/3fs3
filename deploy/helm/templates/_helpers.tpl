{{- define "threefs-s3-gateway.name" -}}
threefs-s3-gateway
{{- end -}}

{{- define "threefs-s3-gateway.fullname" -}}
{{ include "threefs-s3-gateway.name" . }}
{{- end -}}

