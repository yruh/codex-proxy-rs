"""Exercise real HTTP/database isolation in the disposable CI Compose project."""
import datetime
import json
import os
import secrets
import subprocess
import urllib.error
import urllib.parse
import urllib.request

assert os.environ.get("CI") == "true"
project = os.environ["COMPOSE_PROJECT_NAME"]
assert project.startswith("cpr-check-")
base = "http://127.0.0.1:8080"


def request(path, body=None, cookie=None, token=None, expected=200):
    headers = {"Content-Type": "application/json"}
    if cookie:
        headers["Cookie"] = cookie
    if token:
        headers["Authorization"] = "Bearer " + token
    req = urllib.request.Request(base + path, data=None if body is None else json.dumps(body).encode(), headers=headers)
    try:
        response = urllib.request.urlopen(req, timeout=30)
    except urllib.error.HTTPError as error:
        response = error
    assert response.status == expected, f"{path.split('?')[0]}: expected {expected}, got {response.status}"
    data = json.load(response)
    return data.get("data"), response.headers.get("Set-Cookie", "").split(";")[0]


_, admin = request("/api/admin/auth/login", {"username": "admin@cpr.local", "password": os.environ["CI_ADMIN_PASSWORD"]})
assert admin.startswith("cpr_admin_session=")
students = []
for name in ["alice", "bob"]:
    password = secrets.token_urlsafe(24)
    user, _ = request("/api/admin/portal/users", {"username": name, "password": password}, cookie=admin, expected=201)
    _, cookie = request("/api/portal/login", {"username": name, "password": password})
    assert cookie.startswith("cpr_portal_session=")
    students.append((user["id"], cookie))
    data, _ = request("/api/portal/keys", cookie=cookie)
    assert data["items"] == []
    request("/api/admin/portal/users", cookie=cookie, expected=401)
    request("/api/admin/sync/devices", cookie=cookie, expected=401)

# Only synthetic rows in this explicitly isolated CI project; no upstream calls.
sql = """
insert into provider_accounts(id,provider_kind,name,upstream_user_id,authentication_kind,provider_credentials_json,has_refresh_token,access_token_expires_at,credential_state,credential_observed_at,created_at,updated_at,enabled)
values('ci-account','openai','test','test','oauth','{}'::jsonb,false,now()+interval '1 day','ready',now(),now(),now(),false);
insert into client_api_keys(id,name,key,created_at,updated_at)
values('ci-alice','alice','sk_ci_alice',now(),now()),('ci-bob','bob','sk_ci_bob',now(),now());
"""
subprocess.run(["docker", "compose", "--project-name", project, "-f", "deploy/compose.yaml", "exec", "-T", "-e", "PGPASSWORD=" + os.environ["CI_POSTGRES_PASSWORD"], "postgres", "psql", "-v", "ON_ERROR_STOP=1", "-U", "codex_proxy", "-d", "codex_proxy"], input=sql, text=True, check=True, stdout=subprocess.DEVNULL)
for key, (user_id, cookie) in zip(["ci-alice", "ci-bob"], students):
    request("/api/admin/portal/keys/assign", {"keyId": key, "userId": user_id}, cookie=admin)
    data, _ = request("/api/portal/keys", cookie=cookie)
    assert [item["id"] for item in data["items"]] == [key]

device, _ = request("/api/admin/sync/devices", {"name": "CI device", "accountId": "ci-account"}, cookie=admin)
token = device["token"]
now = datetime.datetime.now(datetime.timezone.utc)
record = {"recordId": "ci-record", "revision": 1, "occurredAt": now.isoformat(), "inputTokens": 100, "outputTokens": 10, "cachedTokens": 80, "estimatedUsd": "0.123456789012"}
for changed in [1, 0]:
    data, _ = request("/api/sync/usage", {"records": [record]}, token=token)
    assert data["changed"] == changed
query = urllib.parse.urlencode({"startTime": (now - datetime.timedelta(hours=1)).isoformat(), "endTime": (now + datetime.timedelta(hours=1)).isoformat()})
data, _ = request("/api/admin/usage/combined?" + query, cookie=admin)
assert len(data["items"]) == 1
assert data["items"][0]["inputTokens"] == "100"
assert data["items"][0]["estimatedUsd"] == "0.123456789012"
scoped, _ = request("/api/sync/usage/combined?" + query + "&accountId=another-account", token=token)
assert scoped == data
for _, cookie in students:
    own, _ = request("/api/portal/usage?" + query + "&page=1", cookie=cookie)
    assert own["items"] == [], "Local usage must never be charged to students"
record.update(revision=2, excluded=True)
request("/api/sync/usage", {"records": [record]}, token=token)
data, _ = request("/api/admin/usage/combined?" + query, cookie=admin)
assert data["items"] == []
request("/api/admin/portal/users/update", {"id": students[0][0], "enabled": False}, cookie=admin)
request("/api/portal/status", cookie=students[0][1], expected=401)
request("/api/portal/logout", {}, cookie=students[1][1])
request("/api/portal/status", cookie=students[1][1], expected=401)
print("Dual-source HTTP smoke passed: user isolation, revocation, sync replay and tombstones")
