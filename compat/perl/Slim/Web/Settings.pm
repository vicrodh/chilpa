package Slim::Web::Settings;

use strict;
use warnings;
use Chilpa::Web;
use Slim::Web::Pages;
use Slim::Web::HTTP;
use Slim::Web::HTTP::CSRF;

sub new {
    my ($class) = @_;
    Slim::Web::Pages->addPageFunction($class->page, $class);
    return bless {}, $class;
}

sub handler {
    my ($class, $client, $params) = @_;
    if ($class->can('prefs')) {
        my ($prefs, @keys) = $class->prefs;
        for my $key (@keys) {
            my $field = "pref_$key";
            $prefs->set($key, $params->{$field}) if $params->{saveSettings} && exists $params->{$field};
        }
        # Guest templates use both the preference key and its form-name alias.
        $params->{prefs} = { map { ($_ => $prefs->get($_), "pref_$_" => $prefs->get($_)) } @keys };
    }
    $class->beforeRender($params, $client) if $class->can('beforeRender');
    return Slim::Web::HTTP::filltemplatefile($class->page, $params);
}

1;
