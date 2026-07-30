#!/bin/bash
# Reference benchmark job for the workspace signpost intervals.
set -euo pipefail
cd "$(dirname "$0")/.."

configuration=${CONFIGURATION:-Release}
derived_data_path=${DERIVED_DATA_PATH:-DerivedData}
capture_window=${CAPTURE_WINDOW:-5s}
pass_gap_seconds=${PASS_GAP_SECONDS:-6}
workspace_a=${WORKSPACE_A:-}
workspace_b=${WORKSPACE_B:-}

if [ -z "$workspace_a" ] || [ -z "$workspace_b" ]; then
    echo "Usage: WORKSPACE_A=<uuid-or-path> WORKSPACE_B=<uuid-or-path> Scripts/bench-workspace.sh"
    echo "WORKSPACE_A and WORKSPACE_B must identify two already-open worktrees."
    exit 2
fi
if [ "$workspace_a" = "$workspace_b" ]; then
    echo "WORKSPACE_A and WORKSPACE_B must be different worktrees."
    exit 2
fi

echo "==> Release app build"
xcodebuild -project Tiller.xcodeproj -scheme Tiller \
    -configuration "$configuration" -derivedDataPath "$derived_data_path" \
    CODE_SIGNING_ALLOWED=NO -skipPackagePluginValidation -skipMacroValidation \
    build | tail -5

echo "==> Release tillerctl build"
tillerctl_bin=$(swift build --package-path Packages/TillerControl \
    --configuration release --show-bin-path)/tillerctl

echo "==> Enabling signposts in the app UserDefaults domain"
defaults write dev.tiller.Tiller debug.signpostMetrics -bool YES
cat <<'NOTICE'
Signposts are now enabled in dev.tiller.Tiller.
Quit Tiller and launch it freshly before continuing; SignpostMetrics.enabled is
read once per process by a lazy static let. This script does not launch or quit
the app for you.
NOTICE

if [ "${BENCH_ASSUME_APP_READY:-0}" != "1" ]; then
    if [ ! -t 0 ]; then
        echo "Set BENCH_ASSUME_APP_READY=1 only after freshly launching Tiller." >&2
        exit 2
    fi
    read -r -p "Freshly launched Tiller and opened both worktrees? Press Enter to continue. " _
fi

log_dir=$(mktemp -d /tmp/tiller-workspace-bench.XXXXXX)
trap 'rm -rf "$log_dir"' EXIT

echo "==> Scenario: five warm passes; each pass selects WORKSPACE_A, waits 1s,"
echo "    selects WORKSPACE_B, waits 2s for reconciliation, then captures a"
echo "    separate ${capture_window} signpost window. A ${pass_gap_seconds}s gap"
echo "    separates captures so each log represents one pass."
echo "    Existing tillerctl automation covers worktree selection only; there is"
echo "    no CLI verb for pointer drag, divider tracking, or direct layout commands."
echo "    This script does not invent one: PF-1/PF-3/PF-4 need manual GUI actions"
echo "    during a real-hardware capture if those intervals are to receive samples."

run_logs=()
for pass in 1 2 3 4 5; do
    echo "==> Warm pass $pass/5"
    "$tillerctl_bin" select-workspace --workspace "$workspace_a"
    sleep 1
    "$tillerctl_bin" select-workspace --workspace "$workspace_b"
    sleep 2

    log_file="$log_dir/run-$pass.log"
    log show --predicate 'subsystem == "dev.tiller"' --last "$capture_window" \
        --signpost --debug --info --style compact > "$log_file"
    run_logs+=("$log_file")
    sleep "$pass_gap_seconds"
done

echo "==> Report (the median run is selected independently for each interval)"
perl - "${run_logs[@]}" <<'PERL'
use strict;
use warnings;
use File::Basename qw(basename);
use Time::Piece;

my @intervals = qw(
    workspaceCommandApply
    workspaceReconcile
    workspaceDragFrame
    workspaceStructuralCommit
    workspaceRestore
);
my %allowed = map { $_ => 1 } @intervals;
my %samples;

sub timestamp_from_line {
    my ($line) = @_;
    return unless $line =~ /^(\d{4}-\d{2}-\d{2} \d{2}:\d{2}:\d{2})(?:\.(\d+))?/;
    my $epoch = Time::Piece->strptime($1, "%Y-%m-%d %H:%M:%S")->epoch;
    my $fraction = defined($2) ? "0.$2" : 0;
    return $epoch + $fraction;
}

sub percentile {
    my ($values, $p) = @_;
    my @sorted = sort { $a <=> $b } @$values;
    return 0 unless @sorted;
    my $position = ($#sorted) * $p;
    my $lower = int($position);
    my $upper = $lower < $#sorted ? $lower + 1 : $lower;
    my $weight = $position - $lower;
    return $sorted[$lower] + ($sorted[$upper] - $sorted[$lower]) * $weight;
}

for my $file (@ARGV) {
    open my $fh, '<', $file or die "cannot read $file: $!\n";
    my $run = basename($file);
    my %begins;
    while (my $line = <$fh>) {
        my $name;
        for my $candidate (@intervals) {
            if ($line =~ /\Q$candidate\E/) {
                $name = $candidate;
                last;
            }
        }
        next unless $name && $allowed{$name};
        my $timestamp = timestamp_from_line($line);
        next unless defined $timestamp;

        if ($line =~ /(?:interval\s+)?begin\b|\bbeginInterval\b/i) {
            push @{$begins{$name}}, $timestamp;
        } elsif ($line =~ /(?:interval\s+)?end\b|\bendInterval\b/i) {
            next unless @{$begins{$name} // []};
            my $start = shift @{$begins{$name}};
            my $duration_ms = ($timestamp - $start) * 1000;
            push @{$samples{$run}{$name}}, $duration_ms if $duration_ms >= 0;
        }
    }
    close $fh;
}

sub status_for {
    my ($name, $values, $p95, $p99) = @_;
    return "NO DATA" unless @$values;
    if ($name eq 'workspaceCommandApply') {
        return $p95 <= 1 ? "PASS" : "FAIL";
    } elsif ($name eq 'workspaceReconcile') {
        return $p95 <= 8 ? "PASS" : "FAIL";
    } elsif ($name eq 'workspaceDragFrame') {
        my $consecutive_over_budget = 0;
        for my $index (1 .. $#$values) {
            if ($values->[$index - 1] > 33.3 && $values->[$index] > 33.3) {
                $consecutive_over_budget = 1;
                last;
            }
        }
        return $p95 <= 16.7 && !$consecutive_over_budget ? "PASS" : "FAIL";
    } elsif ($name eq 'workspaceStructuralCommit') {
        return $p95 <= 100 && $p99 <= 250 ? "PASS" : "FAIL";
    } elsif ($name eq 'workspaceRestore') {
        return $p95 <= 250 ? "PASS" : "FAIL";
    }
    return "FAIL";
}

for my $name (@intervals) {
    my @runs = grep { @{$samples{$_}{$name} // []} } keys %samples;
    if (!@runs) {
        print "$name | samples=0 | median=TODO | p95=TODO | NO DATA\n";
        next;
    }
    @runs = sort {
        percentile($samples{$a}{$name}, 0.95) <=>
        percentile($samples{$b}{$name}, 0.95)
    } @runs;
    my $median_run = $runs[int(@runs / 2)];
    my $values = $samples{$median_run}{$name};
    my $median = percentile($values, 0.50);
    my $p95 = percentile($values, 0.95);
    my $p99 = percentile($values, 0.99);
    my $status = status_for($name, $values, $p95, $p99);
    my $suffix = $name eq 'workspaceStructuralCommit'
        ? sprintf(" | p99=%.3f ms", $p99) : "";
    $suffix .= " | median-run=" . $median_run;
    print sprintf(
        "%s | samples=%d | median=%.3f ms | p95=%.3f ms%s | %s\n",
        $name, scalar(@$values), $median, $p95, $suffix, $status
    );
}
PERL
