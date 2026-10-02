package Slim::Utils::Prefs;

use strict;
use warnings;
use Exporter 'import';
use JSON::PP;

our @EXPORT = qw(preferences);
our ($LOAD, $SAVE);
our %SPACES;

sub preferences {
    my ($namespace) = @_;
    die 'preference storage is unavailable' unless $LOAD && $SAVE;
    return $SPACES{$namespace} ||= bless {
        namespace => $namespace, values => decode_json(encode_json($LOAD->($namespace) || {})),
        changes => {}, validators => {}, clients => {},
    }, __PACKAGE__;
}

sub get { $_[0]{values}{$_[1]} }
sub save { $_[0]->persist }

sub persist {
    my ($self) = @_;
    # I send a detached snapshot to the host's encrypted storage service.
    # No file paths or plaintext persistence are exposed by this adapter.
    my $snapshot = decode_json(encode_json($self->{values}));
    die 'preference storage failed' unless $SAVE->($self->{namespace}, $snapshot);
}

sub init {
    my ($self, $defaults) = @_;
    my $changed;
    for my $key (keys %$defaults) {
        next if exists $self->{values}{$key};
        $self->{values}{$key} = $defaults->{$key};
        $changed = 1;
    }
    $self->persist if $changed;
}

sub set {
    my ($self, $key, $value) = @_;
    my $validator = $self->{validators}{$key};
    die 'invalid preference value' if $validator && !$validator->($key, $value, $self->{client});
    my $existed = exists $self->{values}{$key};
    my $previous = $self->{values}{$key};
    $self->{values}{$key} = $value;
    my $saved = eval { $self->persist; 1 };
    if (!$saved) {
        $existed ? ($self->{values}{$key} = $previous) : delete($self->{values}{$key});
        die 'preference storage failed';
    }
    $_->($key, $value, $self->{client}) for @{ $self->{changes}{$key} || [] };
    return $value;
}

sub remove {
    my ($self, @keys) = @_;
    delete @{ $self->{values} }{@keys};
    $self->persist;
}

sub migrate {
    my ($self, $version, $callback) = @_;
    die 'invalid preference migration' unless $version =~ /^\d+$/ && ref($callback) eq 'CODE';
    my $key = "migration:$self->{namespace}";
    my $state = $LOAD->($key) || {};
    return 1 if ($state->{version} || 0) >= $version;
    return unless $callback->($self);
    die 'preference migration storage failed' unless $SAVE->($key, { version => 0 + $version });
    return 1;
}

sub setChange {
    my ($self, $callback, @keys) = @_;
    die 'invalid preference callback' unless ref($callback) eq 'CODE';
    push @{ $self->{changes}{$_} }, $callback for @keys;
}

sub setValidate {
    my ($self, $definition, @keys) = @_;
    my $callback = ref($definition) eq 'HASH' ? $definition->{validator} : $definition;
    die 'unsupported preference validator' unless ref($callback) eq 'CODE';
    $self->{validators}{$_} = $callback for @keys;
}

sub client {
    my ($self, $client) = @_;
    die 'missing preference client' unless ref $client;
    my $id = $client->id;
    return $self->{clients}{$id} ||= bless {
        namespace => "$self->{namespace}/$id", client => $client,
        values => decode_json(encode_json($LOAD->("$self->{namespace}/$id") || {})),
        changes => $self->{changes}, validators => $self->{validators}, clients => {},
    }, ref($self);
}

1;
