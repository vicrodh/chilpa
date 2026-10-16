package Chilpa::Channel;

use strict;
use warnings;
use JSON::PP;
use IO::Select;

use constant MAX_FRAME => 4 * 1024 * 1024;

sub new {
    my ($class, %args) = @_;
    my $writer = $args{write};
    my $previous = select($writer); $| = 1; select($previous);
    return bless { %args, next_service => 0, pending => {}, buffer => '' }, $class;
}

sub send {
    my ($self, $message) = @_;
    my $bytes = encode_json($message);
    die 'worker message is too large' if length($bytes) > MAX_FRAME;
    my $writer = $self->{write};
    print {$writer} pack('N', length($bytes)), $bytes or die 'worker channel disconnected';
}

sub service {
    my ($self, $operation, $callback) = @_;
    die 'too many pending host services' if keys(%{$self->{pending}}) >= 128;
    my $id = ++$self->{next_service};
    $self->{pending}{$id} = $callback;
    $self->send({type => 'service', id => $id, operation => $operation});
}

# Synchronous preferences can wait for the host while already-issued HTTP
# requests complete. A call arriving during an idle timer's storage write is
# deferred until that write completes; it never reenters the running handler.
sub service_sync {
    my ($self, $operation) = @_;
    my ($finished, $result);
    $self->service($operation, sub { $result = shift; $finished = 1 });
    until ($finished) {
        my $message = $self->read_frame;
        if ($message->{type} eq 'call') {
            die 'too many deferred application calls' if $self->{deferred};
            $self->{deferred} = $message;
            next;
        }
        $self->dispatch_reply($message);
    }
    die 'host service failed' unless $result->{ok};
    return $result->{value};
}

sub dispatch_reply {
    my ($self, $message) = @_;
    die 'invalid host service reply' unless $message->{type} eq 'service_reply' && ref($message->{result}) eq 'HASH';
    my $callback = delete $self->{pending}{$message->{id}} or die 'unknown host service reply';
    $callback->($message->{result});
}

sub receive {
    my ($self) = @_;
    return delete $self->{deferred} if $self->{deferred};
    return $self->read_frame;
}

sub read_frame {
    my ($self) = @_;
    my $reader = $self->{read};
    my $prefix = '';
    while (length($prefix) < 4) {
        $prefix .= read_part($reader, 4 - length($prefix));
    }
    my $length = unpack('N', $prefix);
    die 'invalid worker message length' if !$length || $length > MAX_FRAME;
    my $bytes = '';
    while (length($bytes) < $length) {
        $bytes .= read_part($reader, $length - length($bytes));
    }
    my $message = eval { JSON::PP->new->utf8->max_depth(64)->decode($bytes) };
    die 'invalid worker message' unless ref($message) eq 'HASH' && ($message->{type} || '') =~ /^(?:call|service_reply)$/
        && defined($message->{id}) && $message->{id} =~ /^\d+$/ && $message->{id} > 0;
    return $message;
}

sub read_part {
    my ($reader, $length) = @_;
    my $part;
    # Buffered reads could hide the next frame from select while an HTTP
    # callback is pending. Scalar handles are used only by protocol tests.
    my $descriptor = fileno($reader);
    my $count = defined($descriptor) && $descriptor >= 0
        ? sysread($reader, $part, $length) : read($reader, $part, $length);
    die 'worker channel disconnected' unless $count;
    return $part;
}

sub readable {
    my ($self, $timeout) = @_;
    return 1 if $self->{deferred};
    return scalar IO::Select->new($self->{read})->can_read($timeout);
}

1;
