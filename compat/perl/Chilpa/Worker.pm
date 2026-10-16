package Chilpa::Worker;

use strict;
use warnings;
use File::Spec;
use XML::Parser;
use Chilpa::Environment;
use Chilpa::HostServices;
use Chilpa::Web;
use Chilpa::Source;
use Chilpa::Playback;
use Slim::Utils::Strings;
use Slim::Utils::PluginManager;
use Slim::Utils::Timers;
use Slim::Utils::Cache;
use Slim::Utils::Accessor;
use Slim::Networking::Async::HTTP;
use Slim::Menu::GlobalSearch;
use Slim::Menu::TrackInfo;
use Slim::Menu::ArtistInfo;
use Slim::Menu::AlbumInfo;
use Slim::Player::ProtocolHandlers;
use Slim::Formats::RemoteMetadata;

sub new {
    my ($class, $channel, $shared) = @_;
    Chilpa::HostServices->attach($channel);
    return bless { channel => $channel, shared => $shared }, $class;
}

sub initialize {
    my ($self, $input) = @_;
    die 'worker is already initialized' if $self->{web};
    my ($root, $namespace) = @$input{qw(root namespace)};
    die 'invalid plugin namespace' unless $namespace && $namespace =~ /^[A-Za-z_]\w*$/;
    die 'invalid plugin directory' unless defined($root) && -d $root;
    die 'plugin metadata is too large' if -s "$root/install.xml" > 1024 * 1024;
    my (%metadata, @elements);
    XML::Parser->new(Handlers => {
        Start => sub { push @elements, $_[1] },
        End => sub { pop @elements },
        Char => sub { $metadata{$elements[-1]} .= $_[1] if @elements },
        ExternEnt => sub { die 'external XML entities are unsupported' },
        Doctype => sub { die 'XML document types are unsupported' },
    })->parsefile("$root/install.xml");
    $Slim::Utils::PluginManager::NAMESPACE = $namespace;
    $Slim::Utils::PluginManager::INSTALL = \%metadata;
    $Slim::Utils::Strings::LANGUAGE = $input->{language} || 'EN';
    require Digest::SHA;
    my $identity = Digest::SHA::sha256_hex(join(':', time, $$, rand()));
    my $uuid = sprintf('%s-%s-4%s-a%s-%s', substr($identity, 0, 8), substr($identity, 8, 4),
        substr($identity, 13, 3), substr($identity, 17, 3), substr($identity, 20, 12));
    Slim::Utils::Prefs::preferences('server')->init({language => $Slim::Utils::Strings::LANGUAGE,
        server_uuid => $uuid});
    Slim::Utils::Strings->load("$root/strings.txt") if -f "$root/strings.txt";
    $Chilpa::Contributions::ACTIVE = Chilpa::Contributions->new;
    $Chilpa::Web::ACTIVE = $self->{web} = Chilpa::Web->new(
        root => $root, shared => $self->{shared}, strings => \&Slim::Utils::Strings::string,
    );
    # Package paths are mapped to the single installation already mounted by
    # the host. A request for another plugin receives no replacement module.
    unshift @INC, sub {
        my ($hook, $file) = @_;
        return unless $file =~ m{^Plugins/\Q$namespace\E/(.+\.pm)$};
        my $relative = $1;
        die 'invalid guest module path' if $relative =~ m{(?:\\|(?:^|/)\.\.(?:/|$))};
        open my $fh, '<', "$root/$relative" or return;
        return $fh;
    };
    unshift @INC, "$root/lib" if -d "$root/lib";
    require "Plugins/$namespace/Plugin.pm";
    my $class = "Plugins::${namespace}::Plugin";
    $class->initPlugin;
    $self->{source} = Chilpa::Source->new;
    return $self->inspect;
}

sub inspect {
    my ($self) = @_;
    return {
        features => $Chilpa::Contributions::ACTIVE->features,
        settings_pages => [sort keys %{$self->{web}{pages}}],
        limitations => [sort keys %Chilpa::Environment::LIMITATIONS],
        diagnostics => {%Slim::Utils::Log::COUNTS},
        last_failure => $self->{last_failure},
    };
}

sub dispatch {
    my ($self, $message) = @_;
    if ($message->{type} eq 'service_reply') {
        my $ok = eval { $self->{channel}->dispatch_reply($message); 1 };
        $self->failed('callback', $@) unless $ok;
        return;
    }
    die 'worker already has a pending call' if $self->{pending};
    $self->{pending} = $message->{id};
    my $done = $self->{done} = sub {
        return unless defined($self->{pending}) && $self->{pending} == $message->{id};
        delete $self->{done};
        delete $self->{pending};
        $self->{channel}->send({type => 'reply', id => $message->{id}, result => $_[0]});
    };
    my ($operation, $input) = @$message{qw(operation input)};
    my $ok = eval {
        if ($operation eq 'initialize') { $done->($self->initialize($input)) }
        elsif (!$self->{web}) { die 'worker is not initialized' }
        elsif ($operation eq 'inspect' || $operation eq 'tick') { $done->($self->inspect) }
        elsif ($operation eq 'settings.open') { $self->{web}->open($input->{page}, {}, $done) }
        elsif ($operation eq 'settings.submit') { $self->{web}->submit(@$input{qw(revision action values)}, $done) }
        elsif ($operation eq 'settings.document') { $done->($self->{web}->document($input->{page})) }
        elsif ($operation eq 'source.browse') { $self->{source}->browse($input, $done) }
        elsif ($operation eq 'source.search') { $self->{source}->search($input, $done) }
        elsif ($operation eq 'source.query') { $self->{source}->query($input, $done) }
        elsif ($operation eq 'source.resolve') { Chilpa::Playback->resolve($input, $done) }
        elsif ($operation eq 'source.metadata') { require Chilpa::Metadata; Chilpa::Metadata->read($input, $done) }
        else { $done->({error => 'unsupported worker operation'}) }
        1;
    };
    $self->failed('operation', $@) unless $ok;
}

# Guest exceptions can contain credentials or signed addresses. Only a fixed
# category crosses the boundary, including for asynchronous callbacks.
sub failed {
    my ($self, $phase, $exception) = @_;
    my $kind = $exception =~ /Can't locate object method|Undefined subroutine|Can't locate .* in \@INC/
        ? 'unsupported interface' : 'plugin exception';
    my ($file, $line) = $exception =~ m{(?:^|[/\\])([A-Za-z_][A-Za-z0-9_-]*\.p[ml]) line (\d+)};
    my $location = defined($file) ? " [$file:$line]" : '';
    for my $pair (['expired navigation handle', 'expired navigation'], ['source response is too large', 'oversized listing'],
                  ['Not a HASH reference', 'invalid object'], ['Not an ARRAY reference', 'invalid list']) {
        $kind = $pair->[1] if index($exception, $pair->[0]) >= 0;
    }
    $self->{last_failure} = { phase => $phase, kind => $kind, length($location) ? (location => $location) : () };
    if (my $done = $self->{done}) {
        $done->({error => "Compatibility worker $phase failed: $kind$location."});
    }
}

sub run {
    my ($self) = @_;
    while (1) {
        my $ok = eval { Slim::Utils::Timers::fire_due(time); 1 };
        $self->failed('timer', $@) unless $ok;
        my $next = Slim::Utils::Timers::next_deadline();
        my $wait = defined($next) ? $next - time : 1;
        $wait = 0 if $wait < 0;
        $wait = 1 if $wait > 1;
        $self->dispatch($self->{channel}->receive) if $self->{channel}->readable($wait);
    }
}

1;
