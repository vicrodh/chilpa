package Chilpa::Settings;

use strict;
use warnings;
use Template;
use HTML::TreeBuilder;
use JSON::PP ();

# I render the guest's template with my own shared wrappers, then translate
# controls into data. Scripts, event attributes and guest markup never leave
# this process. Hidden values stay here for the next submitted action.
sub render {
    my ($root, $shared, $page, $params, $strings) = @_;
    die "invalid settings path" if !$page || $page =~ m{(?:^/|\\|(?:^|/)\.\.(?:/|$))};
    my $engine = Template->new({
        INCLUDE_PATH => [$shared, "$root/HTML/EN"],
        ABSOLUTE => 0, RELATIVE => 0, EVAL_PERL => 0,
        FILTERS => { string => sub { $strings->($_[0]) } },
    });
    my $html = '';
    $engine->process($page, $params, \$html) or die "settings template could not be rendered: " . $engine->error;
    die "settings template too large" if length($html) > 1024 * 1024;
    return from_html($html);
}

sub text {
    my ($node) = @_;
    return '' unless $node;
    my $text = ref($node) ? $node->as_text : $node;
    $text =~ s/[\s\x{a0}]+/ /g;
    $text =~ s/^\s+|\s+$//g;
    return $text;
}

sub from_html {
    my ($html) = @_;
    die "settings template too large" if length($html) > 1024 * 1024;
    my $tree = HTML::TreeBuilder->new();
    $tree->parse_content($html);
    $tree->eof;
    my (@fields, @actions, @links, @unsupported, %values, %hidden, %submits, %types, %seen);
    my %labels = map { ($_->attr('for') || '') => text($_) } $tree->look_down(_tag => 'label');
    for my $node ($tree->look_down(sub { $_[0]->tag =~ /^(?:input|select|textarea|button)$/ })) {
        my $name = $node->attr('name') || '';
        next unless length($name);
        my $type = lc($node->attr('type') || ($node->tag eq 'button' ? 'submit' : 'text'));
        if ($type eq 'hidden') {
            $hidden{$name} = $node->attr('value') // '';
            next;
        }
        if ($type eq 'submit') {
            push @actions, { key => $name, label => text($node->attr('value') || $node) };
            $submits{$name} = $node->attr('value') // '1';
            next;
        }
        if ($seen{$name}++) {
            push @unsupported, "repeated control: $name";
            next;
        }
        my $label = $labels{$node->attr('id') || ''} || $node->attr('aria-label') || table_label($node);
        my $description = $node->attr('title') || '';
        my $parent = $node->parent;
        while ($parent) {
            if ($parent->tag eq 'fieldset') {
                my ($legend) = $parent->look_down(_tag => 'legend');
                $label ||= text($legend);
                my ($help) = grep { ref($_) && $_->tag eq 'p' } $parent->content_list;
                $description ||= text($help);
                last;
            }
            $parent = $parent->parent;
        }
        $label ||= $name;
        my $control;
        if ($node->tag eq 'select') {
            if (defined $node->attr('multiple')) {
                push @unsupported, "multiple selection: $name";
                next;
            }
            my @choices;
            for my $option ($node->look_down(_tag => 'option')) {
                my $value = $option->attr('value') // text($option);
                push @choices, { label => text($option), value => "$value" };
                $values{$name} = "$value" if defined $option->attr('selected') || !exists $values{$name};
            }
            $control = { type => 'select', choices => \@choices };
        }
        elsif ($type eq 'checkbox') {
            $control = { type => 'toggle' };
            $values{$name} = defined $node->attr('checked') ? JSON::PP::true : JSON::PP::false;
            $types{$name} = { checked => $node->attr('value') // '1' };
        }
        elsif ($type =~ /^(?:text|password|email|url|search|tel)$/ || $node->tag eq 'textarea') {
            my $secret = $type eq 'password' || $name =~ /password|secret|token|cookie|credential/i;
            $control = { type => $secret ? 'secret' : 'text', max_length => 65536 };
            my $length = $node->attr('maxlength');
            $control->{max_length} = 0 + $length if defined($length) && $length =~ /^\d+$/ && $length > 0 && $length < 65536;
            $values{$name} = $node->tag eq 'textarea' ? text($node) : ($node->attr('value') // '') unless $secret;
        }
        else {
            push @unsupported, "control type: $type";
            next;
        }
        push @fields, {
            key => $name, label => $label, description => $description,
            required => defined $node->attr('required') ? JSON::PP::true : JSON::PP::false,
            control => $control,
        };
    }
    my @keys = map { $_->{key} } @fields;
    $_->{fields} = [@keys] for @actions;
    for my $link ($tree->look_down(_tag => 'a')) {
        my $href = $link->attr('href') || '';
        next if !$href || $href =~ /[\x00-\x20\x7f]/ || $href =~ /^#/;
        next if $href =~ /^([a-z][a-z0-9+.-]*):/i && lc($1) !~ /^https?$/;
        push @links, { label => text($link), url => $href };
    }
    # The host handles page refresh explicitly. I retain the missing script
    # behavior as a limitation instead of treating static rendering as full
    # compatibility with arbitrary web settings.
    push @unsupported, 'scripted settings behavior' if $tree->look_down(_tag => 'script');
    my @notices = map { text($_) } $tree->look_down(role => 'alert');
    my @content;
    for my $node ($tree->look_down(sub { $_[0]->tag =~ /^(?:p|h[1-6])$/ })) {
        my $parent = $node->parent;
        $parent = $parent->parent while $parent && $parent->tag ne 'fieldset';
        next if $parent || ($node->attr('role') || '') eq 'alert';
        my $text = text($node);
        push @content, { kind => $node->tag eq 'p' ? 'paragraph' : 'heading', text => $text } if length $text;
    }
    # Read-only table cells can describe an already connected account beside
    # a remove button. Control values and their labels stay in the form.
    my %field_labels = map { $_->{label} => 1 } @fields;
    for my $row ($tree->look_down(_tag => 'tr')) {
        my @parts;
        for my $cell (grep { ref($_) && $_->tag =~ /^(?:td|th)$/ } $row->content_list) {
            next if $cell->look_down(sub { $_[0]->tag =~ /^(?:input|select|textarea|button|script|style|table)$/ });
            my $value = text($cell);
            push @parts, $value if length($value) && !$field_labels{$value};
        }
        push @content, { kind => 'paragraph', text => join(' · ', @parts) } if @parts;
    }
    $tree->delete;
    die "too many settings controls" if @fields > 128 || @actions > 128;
    return {
        public => { form => { fields => \@fields, actions => \@actions, unsupported => \@unsupported }, values => \%values, links => \@links, notices => \@notices, content => \@content },
        private => { hidden => \%hidden, submits => \%submits, types => \%types },
    };
}

sub table_label {
    my ($node) = @_;
    my $cell = $node->parent;
    $cell = $cell->parent while $cell && $cell->tag !~ /^(?:td|th)$/;
    return '' unless $cell && $cell->parent && $cell->parent->tag eq 'tr';
    my $row = $cell->parent;
    my @cells = grep { ref($_) && $_->tag =~ /^(?:td|th)$/ } $row->content_list;
    my ($index) = grep { $cells[$_] == $cell } 0 .. $#cells;
    my $table = $row->parent;
    $table = $table->parent while $table && $table->tag ne 'table';
    return '' unless $table && defined $index;
    my @before;
    for my $candidate ($table->look_down(_tag => 'tr')) {
        last if $candidate == $row;
        push @before, $candidate;
    }
    for my $candidate (reverse @before) {
        my @headers = grep { ref($_) && $_->tag =~ /^(?:td|th)$/ } $candidate->content_list;
        my $header = $headers[$index] or next;
        next if $header->look_down(sub { $_[0]->tag =~ /^(?:input|select|button|textarea)$/ });
        my $label = text($header);
        return $label if length $label;
    }
    return '';
}

# The caller validates the public submission against the typed form first.
# Private hidden fields are restored from the rendered form, not from QML.
sub submission {
    my ($rendered, $action, $values) = @_;
    my $private = $rendered->{private};
    die "unknown settings action" unless exists $private->{submits}{$action};
    my %allowed = map { $_->{key} => 1 } @{ $rendered->{public}{form}{fields} };
    my %params = %{ $private->{hidden} };
    for my $key (keys %$values) {
        die "unknown settings field" unless $allowed{$key};
        my $type = $private->{types}{$key};
        $params{$key} = $type ? ($values->{$key} ? $type->{checked} : 0) : $values->{$key};
    }
    $params{$action} = $private->{submits}{$action};
    return \%params;
}

1;
