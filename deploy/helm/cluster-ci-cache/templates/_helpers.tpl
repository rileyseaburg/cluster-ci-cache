{{- define "cluster-ci-cache.labels" -}}
app.kubernetes.io/name: cluster-ci-cache
app.kubernetes.io/instance: {{ .Release.Name }}
app.kubernetes.io/managed-by: {{ .Release.Service }}
app.kubernetes.io/part-of: cluster-ci-cache
helm.sh/chart: {{ .Chart.Name }}-{{ .Chart.Version }}
{{- end -}}

{{- define "cluster-ci-cache.image" -}}
{{ .Values.image.registry }}/{{ .Values.image.repository }}:{{ .Values.image.tag }}
{{- end -}}

{{- define "cluster-ci-cache.configYaml" -}}
server:
  listen_addr: {{ .Values.server.listenAddr | quote }}
backend:
  kind: {{ .Values.backend.kind | quote }}
  fs_root: {{ .Values.backend.fsRoot | quote }}
  bucket: {{ .Values.backend.bucket | quote }}
  endpoint: {{ .Values.backend.endpoint | quote }}
  region: {{ .Values.backend.region | quote }}
  access_key_env: {{ .Values.backend.accessKeyEnv | quote }}
  secret_key_env: {{ .Values.backend.secretKeyEnv | quote }}
  path_style: {{ .Values.backend.pathStyle }}
cache:
  default_ttl_seconds: {{ .Values.cache.defaultTtlSeconds }}
  compression: {{ .Values.cache.compression | quote }}
  max_blob_size_mb: {{ .Values.cache.maxBlobSizeMB }}
  max_archive_size_gb: {{ .Values.cache.maxArchiveSizeGB }}
  dedupe: {{ .Values.cache.dedupe }}
auth:
  mode: {{ .Values.auth.mode | quote }}
  token_env: {{ .Values.auth.tokenEnv | quote }}
{{- end -}}
