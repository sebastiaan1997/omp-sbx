package main

import (
	"encoding/json"
	"errors"
	"flag"
	"fmt"
	"net"
	"net/http"
	"net/url"
	"os"
	"path/filepath"
	"regexp"
	"strings"
)

type policy struct {
	SchemaVersion int      `json:"schemaVersion"`
	ID            string   `json:"id"`
	Images        []string `json:"images"`
}

type authzRequest struct {
	RequestMethod string              `json:"RequestMethod"`
	RequestURI    string              `json:"RequestURI"`
	RequestBody   []byte              `json:"RequestBody"`
	RequestHeader map[string][]string `json:"RequestHeader"`
}

type authzResponse struct {
	Allow bool   `json:"Allow"`
	Msg   string `json:"Msg,omitempty"`
	Err   string `json:"Err,omitempty"`
}

type server struct {
	policy  policy
	allowed map[string]struct{}
}

var (
	apiPrefix     = regexp.MustCompile(`^/v[0-9]+(?:\.[0-9]+)?`)
	hexImageID    = regexp.MustCompile(`^(?:sha256:)?[0-9a-f]{64}$`)
	nameComponent = regexp.MustCompile(`^[a-z0-9]+(?:[._-][a-z0-9]+)*$`)
	registryPart  = regexp.MustCompile(`^[a-z0-9]+(?:[.-][a-z0-9]+)*(?::[0-9]+)?$`)
	tagPart       = regexp.MustCompile(`^[A-Za-z0-9_][A-Za-z0-9_.-]{0,127}$`)
	digestPart    = regexp.MustCompile(`^sha256:[0-9a-f]{64}$`)
	serviceUpdate = regexp.MustCompile(`^/services/[^/]+/update$`)
	imageTagPath  = regexp.MustCompile(`^/images/.+/tag$`)
	imagePushPath = regexp.MustCompile(`^/images/.+/push$`)
)

func main() {
	policyPath := flag.String("policy", "", "compiled policy.json")
	socketPath := flag.String("socket", "/run/omp-sbx/image-authz.sock", "plugin Unix socket")
	flag.Parse()
	if *policyPath == "" {
		fatal(errors.New("--policy is required"))
	}

	contents, err := os.ReadFile(*policyPath)
	if err != nil {
		fatal(err)
	}
	var configured policy
	if err := json.Unmarshal(contents, &configured); err != nil {
		fatal(fmt.Errorf("decode policy: %w", err))
	}
	if configured.SchemaVersion != 1 || configured.ID == "" {
		fatal(errors.New("unsupported or incomplete policy"))
	}
	allowed := make(map[string]struct{}, len(configured.Images))
	for _, image := range configured.Images {
		allowed[image] = struct{}{}
	}

	if err := os.MkdirAll(filepath.Dir(*socketPath), 0o755); err != nil {
		fatal(err)
	}
	if err := os.RemoveAll(*socketPath); err != nil {
		fatal(err)
	}
	listener, err := net.Listen("unix", *socketPath)
	if err != nil {
		fatal(err)
	}
	if err := os.Chmod(*socketPath, 0o660); err != nil {
		_ = listener.Close()
		fatal(err)
	}

	implementation := &server{policy: configured, allowed: allowed}
	mux := http.NewServeMux()
	mux.HandleFunc("/Plugin.Activate", implementation.activate)
	mux.HandleFunc("/AuthZPlugin.AuthZReq", implementation.authorizeRequest)
	mux.HandleFunc("/AuthZPlugin.AuthZRes", implementation.authorizeResponse)
	if err := http.Serve(listener, mux); err != nil && !errors.Is(err, net.ErrClosed) {
		fatal(err)
	}
}

func fatal(err error) {
	fmt.Fprintf(os.Stderr, "omp-sbx image AuthZ: %v\n", err)
	os.Exit(1)
}

func writeJSON(w http.ResponseWriter, value any) {
	w.Header().Set("Content-Type", "application/json")
	if err := json.NewEncoder(w).Encode(value); err != nil {
		fmt.Fprintf(os.Stderr, "omp-sbx image AuthZ: encode response: %v\n", err)
	}
}

func (s *server) activate(w http.ResponseWriter, r *http.Request) {
	if r.Method != http.MethodPost {
		http.Error(w, "method not allowed", http.StatusMethodNotAllowed)
		return
	}
	writeJSON(w, map[string][]string{"Implements": {"authz"}})
}

func (s *server) authorizeResponse(w http.ResponseWriter, r *http.Request) {
	if r.Method != http.MethodPost {
		http.Error(w, "method not allowed", http.StatusMethodNotAllowed)
		return
	}
	writeJSON(w, authzResponse{Allow: true})
}

func (s *server) authorizeRequest(w http.ResponseWriter, r *http.Request) {
	if r.Method != http.MethodPost {
		http.Error(w, "method not allowed", http.StatusMethodNotAllowed)
		return
	}
	var request authzRequest
	decoder := json.NewDecoder(r.Body)
	if err := decoder.Decode(&request); err != nil {
		writeJSON(w, authzResponse{Allow: false, Err: "malformed authorization request"})
		return
	}
	allowed, message := s.evaluate(request)
	writeJSON(w, authzResponse{Allow: allowed, Msg: message})
}

func (s *server) evaluate(request authzRequest) (bool, string) {
	parsed, err := url.ParseRequestURI(request.RequestURI)
	if err != nil {
		return false, "malformed Docker request URI"
	}
	path := apiPrefix.ReplaceAllString(parsed.Path, "")
	if request.RequestMethod != http.MethodPost {
		return true, ""
	}

	switch {
	case path == "/images/create":
		if parsed.Query().Get("fromSrc") != "" {
			return false, "Docker image import is not allowed by omp-sbx policy"
		}
		image := parsed.Query().Get("fromImage")
		if tag := parsed.Query().Get("tag"); image != "" && tag != "" && !strings.Contains(image, "@") {
			lastSlash := strings.LastIndex(image, "/")
			if strings.LastIndex(image, ":") <= lastSlash {
				image += ":" + tag
			}
		}
		return s.externalAllowed(image)
	case path == "/containers/create":
		var body struct {
			Image string `json:"Image"`
		}
		if err := json.Unmarshal(request.RequestBody, &body); err != nil || body.Image == "" {
			return false, "container create has a missing or malformed image reference"
		}
		return s.containerImageAllowed(body.Image)
	case path == "/images/load":
		return false, "Docker image load is not allowed by omp-sbx policy"
	case path == "/commit":
		return false, "Docker commit is not allowed by omp-sbx policy"
	case path == "/plugins/create" || path == "/plugins/pull":
		return false, "Docker plugin images are not allowed by omp-sbx policy"
	case path == "/services/create" || serviceUpdate.MatchString(path):
		return false, "Docker Swarm services are not allowed by omp-sbx policy"
	case path == "/build":
		if headerValue(request.RequestHeader, "X-OMP-SBX-Policy") != s.policy.ID {
			return false, "Docker build requires the omp-sbx Build Policy wrapper"
		}
		return true, ""
	case imageTagPath.MatchString(path):
		repository := parsed.Query().Get("repo")
		if tag := parsed.Query().Get("tag"); tag != "" {
			repository += ":" + tag
		}
		return s.containerImageAllowed(repository)
	case imagePushPath.MatchString(path):
		return true, ""
	case strings.HasPrefix(path, "/images/"):
		return false, "unknown Docker image mutation is not allowed by omp-sbx policy"
	default:
		return true, ""
	}
}

func headerValue(headers map[string][]string, wanted string) string {
	for name, values := range headers {
		if strings.EqualFold(name, wanted) && len(values) > 0 {
			return values[0]
		}
	}
	return ""
}

func (s *server) externalAllowed(reference string) (bool, string) {
	canonical, err := canonicalImage(reference)
	if err != nil {
		return false, fmt.Sprintf("invalid Docker image reference %q", reference)
	}
	if _, ok := s.allowed[canonical]; ok {
		return true, ""
	}
	return false, fmt.Sprintf("image %s is not approved by omp-sbx policy %s", canonical, s.policy.ID)
}

func (s *server) containerImageAllowed(reference string) (bool, string) {
	if reference == "" || hexImageID.MatchString(reference) {
		return false, "container image IDs are not allowed by omp-sbx policy"
	}
	name := reference
	if at := strings.IndexByte(name, '@'); at >= 0 {
		name = name[:at]
	}
	lastSlash := strings.LastIndex(name, "/")
	lastColon := strings.LastIndex(name, ":")
	withoutTag := name
	if lastColon > lastSlash {
		withoutTag = name[:lastColon]
	}
	if !strings.Contains(withoutTag, "/") || strings.HasPrefix(withoutTag, "local/") {
		return true, ""
	}
	return s.externalAllowed(reference)
}

func canonicalImage(reference string) (string, error) {
	if reference == "" || reference != strings.TrimSpace(reference) || strings.ContainsAny(reference, " \t\r\n") || strings.Contains(reference, "://") {
		return "", errors.New("invalid reference")
	}
	if strings.Count(reference, "@") > 1 {
		return "", errors.New("invalid digest")
	}
	nameAndTag, digest, hasDigest := strings.Cut(reference, "@")
	if hasDigest && !digestPart.MatchString(digest) {
		return "", errors.New("invalid digest")
	}
	lastSlash := strings.LastIndex(nameAndTag, "/")
	lastColon := strings.LastIndex(nameAndTag, ":")
	name, tag := nameAndTag, ""
	if lastColon > lastSlash {
		name, tag = nameAndTag[:lastColon], nameAndTag[lastColon+1:]
		if !tagPart.MatchString(tag) {
			return "", errors.New("invalid tag")
		}
	}
	if name == "" || name != strings.ToLower(name) {
		return "", errors.New("invalid repository")
	}
	parts := strings.Split(name, "/")
	registry := "docker.io"
	repository := parts
	if strings.ContainsAny(parts[0], ".:") || parts[0] == "localhost" {
		if !registryPart.MatchString(parts[0]) || len(parts) < 2 {
			return "", errors.New("invalid registry")
		}
		registry, repository = parts[0], parts[1:]
	}
	if registry == "docker.io" && len(repository) == 1 {
		repository = append([]string{"library"}, repository...)
	}
	for _, component := range repository {
		if !nameComponent.MatchString(component) {
			return "", errors.New("invalid repository")
		}
	}
	canonical := registry + "/" + strings.Join(repository, "/")
	if tag != "" {
		canonical += ":" + tag
	} else if !hasDigest {
		canonical += ":latest"
	}
	if hasDigest {
		canonical += "@" + digest
	}
	return canonical, nil
}
